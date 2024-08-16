mod aes_gcm;
mod base64_blobs;
mod content_range;
mod ed448;
mod encrypted;
mod id;
mod identity;
mod key;
mod lock;
mod nodes;
mod public_key;
mod purge;
mod salt;
mod sessions;
mod shares;
mod users;
mod webauthn;
mod x448;

use crate::purge::Purge;
use axum::{
	body::{Body, BodyDataStream},
	extract::{self, Path, Request},
	http::{HeaderMap, HeaderValue, Response as HttpResponse, StatusCode},
	response::{IntoResponse, Response},
	routing::{delete, get, head, post},
	Json, Router,
};
use axum_server::{tls_rustls::RustlsConfig, Server};
use content_range::{ContentRange, Range};
use futures_util::StreamExt;
use id::Uid;
use nodes::LockedNode;
use nodes::Nodes;
use sessions::Sessions;
use shares::{Invite, Shares, Welcome};
use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::{fs::OpenOptions, sync::Mutex};
use tower_http::cors::CorsLayer;
use users::{LockedUser, Login, Signup, Users};
use webauthn::Webauthn;

// Define a custom error type that can convert into an HTTP response
#[derive(Debug)]
enum Error {
	Io(String),
	Unauthorised,
	InvalidRange,
	NotFound(Uid),
	NoInvite(String),
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		Error::Io(format!("{}", err))
	}
}

impl From<hyper::Error> for Error {
	fn from(err: hyper::Error) -> Self {
		Error::Io(format!("{}", err))
	}
}

impl From<axum::Error> for Error {
	fn from(err: axum::Error) -> Self {
		Error::Io(format!("{}", err))
	}
}

impl IntoResponse for Error {
	fn into_response(self) -> Response {
		match self {
			Error::Io(_) => StatusCode::SERVICE_UNAVAILABLE,
			Error::Unauthorised => StatusCode::FORBIDDEN,
			Error::InvalidRange => StatusCode::RANGE_NOT_SATISFIABLE,
			Error::NotFound(_) => StatusCode::NOT_FOUND,
			Error::NoInvite(_) => StatusCode::NOT_FOUND,
		}
		.into_response()
	}
}

#[derive(Clone)]
struct State {
	nodes: Arc<Mutex<Nodes>>,
	shares: Arc<Mutex<Shares>>,
	users: Arc<Mutex<Users>>,
	sessions: Arc<Mutex<Sessions>>,
	webauthn: Arc<Mutex<Webauthn>>,
}

impl State {
	fn new() -> Self {
		Self {
			nodes: Arc::new(Mutex::new(Nodes::new())),
			shares: Arc::new(Mutex::new(Shares::new())),
			users: Arc::new(Mutex::new(Users::new())),
			sessions: Arc::new(Mutex::new(Sessions::new())),
			webauthn: Arc::new(Mutex::new(Webauthn::new())),
		}
	}

	async fn purge(&mut self) {
		{
			self.nodes.lock().await.purge();
		}
		{
			self.shares.lock().await.purge();
		}
		{
			self.users.lock().await.purge();
		}
		{
			self.sessions.lock().await.purge();
		}
		{
			self.webauthn.lock().await.purge();
		}
	}

	async fn user_by_email(&self, email: &str) -> Result<LockedUser, Error> {
		println!("getting user with email: {}", email);

		let id = self
			.users
			.lock()
			.await
			.id_for_email(&email)
			.ok_or(Error::Unauthorised)?;

		self.user_by_id(id).await
	}

	async fn user_by_id(&self, id: Uid) -> Result<LockedUser, Error> {
		let nodes = self.nodes.lock().await;
		let shares = self.shares.lock().await;
		let users = self.users.lock().await;

		println!("getting user by id: {:?}", id);

		let _priv = users.priv_for_id(id).ok_or(Error::Unauthorised)?;
		let _pub = users.pub_for_id(id).ok_or(Error::Unauthorised)?;
		let shares = shares.all_shares_for_user(id);
		// FIXME: return nodes based on exports and pending uploads (if uploader == users.id_for_email(email))
		let roots = nodes.get_all();

		Ok(LockedUser {
			encrypted_priv: _priv.clone(),
			_pub: _pub.clone(),
			shares,
			roots,
		})
	}
}

async fn open_file_at_offset(
	file_id: Uid,
	create: bool,
	append: bool,
	read: bool,
	write: bool,
	offset: u64,
) -> Result<tokio::fs::File, Error> {
	let path = path_for_file_id(file_id);

	println!("path to process: {}", path);

	let mut file = OpenOptions::new()
		.create(create)
		.append(append)
		.read(read)
		.write(write)
		.open(path)
		.await
		.map_err(|e| Error::from(e))?;

	file.seek(tokio::io::SeekFrom::Start(offset)).await?;

	Ok(file)
}

async fn check_auth(headers: &HeaderMap) -> Result<(), Error> {
	if headers
		.get("x-uploader-auth")
		.eq(&Some(&HeaderValue::from_bytes(b"aabb1122").unwrap()))
	{
		Ok(())
	} else {
		Err(Error::Unauthorised)
	}
}

async fn process_data_stream(
	file_id: Uid,
	mut file: tokio::fs::File,
	mut stream: BodyDataStream,
) -> Result<StatusCode, Error> {
	println!("auth passed, working...");

	while let Some(chunk) = stream.next().await {
		let data = chunk?;

		file.write_all(&data).await?;
		println!("{}: chunk size - {}", file_id.to_base64(), data.len());
	}

	Ok(StatusCode::OK)
}

async fn handle_upload(
	file_id: Uid,
	request: Request<Body>,
	append: bool,
) -> Result<StatusCode, Error> {
	check_auth(&request.headers()).await?;

	let range = request
		.headers()
		.get("Content-Range")
		.and_then(|header| header.to_str().ok())
		.and_then(|header_str| ContentRange::from_str(header_str).ok())
		.ok_or(Error::InvalidRange)?;

	println!("received: {}", range.to_string());

	let file = open_file_at_offset(file_id, true, append, false, true, range.start).await?;
	let stream = request.into_body().into_data_stream();

	process_data_stream(file_id, file, stream).await
}

async fn upload_stream(
	Path(file_id): Path<Uid>,
	request: Request<Body>,
) -> Result<StatusCode, Error> {
	handle_upload(file_id, request, false).await
}

async fn upload_ranged(
	Path(file_id): Path<Uid>,
	request: Request<Body>,
) -> Result<StatusCode, Error> {
	handle_upload(file_id, request, false).await
}

async fn read_file_chunk(file_id: Uid, start: u64, end: u64) -> Result<Vec<u8>, Error> {
	let mut file = open_file_at_offset(file_id, false, false, true, false, start).await?;
	let mut buffer = vec![0; (end - start + 1) as usize];

	file.read_exact(&mut buffer).await?;

	Ok(buffer)
}

async fn download_ranged(
	Path(file_id): Path<Uid>,
	request: Request<Body>,
) -> Result<Response<Body>, Error> {
	check_auth(&request.headers()).await?;

	let range = request
		.headers()
		.get("Range")
		.and_then(|header| header.to_str().ok())
		.and_then(|header_str| Range::from_str(header_str).ok())
		.ok_or(Error::InvalidRange)?;

	let chunk = read_file_chunk(file_id, range.start, range.end).await?;

	let response = Response::builder()
		.status(StatusCode::PARTIAL_CONTENT)
		.header(
			"Content-Range",
			ContentRange {
				start: range.start,
				end: range.end,
				length: Some(chunk.len() as u64),
			}
			.to_string(),
		)
		.header("Content-Length", chunk.len())
		.body(Body::from(chunk))
		.unwrap();

	Ok(response)
}

async fn file_length(file_path: String) -> Option<usize> {
	// FIXME: check auth token
	if let Ok(file) = OpenOptions::new().read(true).open(file_path).await {
		if let Ok(metadata) = file.metadata().await {
			return Some(metadata.len() as usize);
		}
	}
	None
}

async fn check_file_length(Path(file_id): Path<Uid>) -> Result<HttpResponse<Body>, Error> {
	let file_path = format!("uploads/{}", file_id.to_base64());

	match file_length(file_path).await {
		Some(length) => Ok(Response::builder()
			.status(StatusCode::OK)
			.header("Content-Length", length.to_string())
			.body(Body::empty())
			.unwrap()),
		None => Err(Error::Io("File not found".to_string())),
	}
}

async fn add_nodes(
	extract::State(state): extract::State<State>,
	extract::Json(new_nodes): extract::Json<Vec<LockedNode>>,
) -> Result<StatusCode, Error> {
	let mut nodes = state.nodes.lock().await;

	new_nodes.into_iter().for_each(|n| {
		println!("inserting {}", n.id.to_base64());

		nodes.add(n);
	});

	Ok(StatusCode::CREATED)
}

async fn signup(
	extract::State(state): extract::State<State>,
	extract::Json(signup): extract::Json<Signup>,
) -> Result<StatusCode, Error> {
	let mut nodes = state.nodes.lock().await;
	let mut shares = state.shares.lock().await;
	let mut users = state.users.lock().await;
	let user = signup.user;
	let user_id = user._pub.id();

	user.roots.iter().for_each(|node| {
		nodes.add(node.clone());
	});

	user.shares.iter().for_each(|share| {
		shares.add_share(share.clone());
	});
	shares.delete_invite(&signup.email);

	users.add_priv(user_id, user.encrypted_priv);
	users.add_pub(user_id, user._pub);
	// password should be hashed and stored as well, but no need for now
	users.add_credentials(&signup.email, user_id);

	println!("signed up {}", signup.email);

	// you'd generate an access token here for subsequent requests

	Ok(StatusCode::CREATED)
}

async fn login(
	extract::State(state): extract::State<State>,
	extract::Json(login): extract::Json<Login>,
) -> Result<(StatusCode, Json<LockedUser>), Error> {
	println!("loggin in via email/pass: {}", login.email);

	let user = state.user_by_email(&login.email).await?;

	println!("logged in {}", login.email);

	// you'd generate an access token here for subsequent requests

	Ok((StatusCode::OK, Json(user)))
}

async fn get_invite(
	extract::State(state): extract::State<State>,
	Path(email_base64): Path<String>,
) -> Result<(StatusCode, Json<Welcome>), Error> {
	let shares = state.shares.lock().await;
	let nodes = state.nodes.lock().await;

	println!("getting invite: {}", email_base64);

	let email = String::from_utf8(
		base64::decode_config(&email_base64, base64::URL_SAFE)
			.map_err(|_| Error::NoInvite(email_base64.clone()))?,
	)
	.map_err(|_| Error::NoInvite(email_base64.clone()))?;

	println!("getting invite decoded: {}", email);

	if let Some(invite) = shares.invie_for_mail(&email) {
		let welcome = Welcome {
			user_id: invite.user_id,
			sender: invite.sender.clone(),
			imports: invite.payload.clone(),
			sig: invite.sig.clone(),
			// FIXME: return nodes based on exports and pending uploads (if uploader == users.id_for_email(email))
			nodes: nodes.get_all(),
		};

		// TODO: do I need thi sstatus code?
		Ok((StatusCode::OK, Json(welcome)))
	} else {
		Err(Error::NoInvite(email_base64))
	}
}

async fn get_master_key(
	extract::State(state): extract::State<State>,
	Path(user_id): Path<Uid>,
) -> Result<(StatusCode, Json<encrypted::Encrypted>), Error> {
	let users = state.users.lock().await;

	println!("getting mk: {}", user_id.to_base64());

	if let Some(mk) = users.mk_for_id(user_id) {
		Ok((StatusCode::OK, Json(mk.clone())))
	} else {
		Err(Error::NotFound(user_id))
	}
}

async fn get_user(
	extract::State(state): extract::State<State>,
	Path(user_id): Path<Uid>,
) -> Result<(StatusCode, Json<LockedUser>), Error> {
	let user = state.user_by_id(user_id).await?;

	println!("logged in {}", user_id.to_base64());

	// you'd generate an access token here for subsequent requests

	Ok((StatusCode::OK, Json(user)))
}

async fn invite(
	extract::State(state): extract::State<State>,
	extract::Json(invite): extract::Json<Invite>,
) -> Result<StatusCode, Error> {
	let mut shares = state.shares.lock().await;
	let email = invite.email.clone();

	println!("inviting: {}", email);

	shares.add_invite(invite);

	Ok(StatusCode::CREATED)
}

async fn lock_session(
	extract::State(state): extract::State<State>,
	Path(token_id): Path<Uid>,
	extract::Json(token): extract::Json<shares::Seed>,
) -> Result<StatusCode, Error> {
	let mut sessions = state.sessions.lock().await;

	println!("locking session: {}", token_id.to_base64());

	sessions.add_token(token_id, token);

	Ok(StatusCode::CREATED)
}

async fn unlock_session(
	extract::State(state): extract::State<State>,
	Path(token_id): Path<Uid>,
) -> Result<(StatusCode, Json<shares::Seed>), Error> {
	let mut sessions = state.sessions.lock().await;

	// should be authenticated probably; on another hand,
	// session id is already supplied which should be enough, should it not?
	println!("unlocking session: {}", token_id.to_base64());

	if let Some(token) = sessions.consume_token_by_id(token_id) {
		Ok((StatusCode::OK, Json(token)))
	} else {
		// TODO: a different error code or return a random token?
		Err(Error::Unauthorised)
	}
}

async fn delete_node(
	extract::State(state): extract::State<State>,
	Path(file_id): Path<Uid>,
) -> Result<StatusCode, Error> {
	if let Some(_) = state.nodes.lock().await.remove(file_id) {
		remove_file(file_id).await;

		println!("deleted {}", file_id.to_base64());

		Ok(StatusCode::NO_CONTENT)
	} else {
		println!("can not delete {}; not found", file_id.to_base64());

		Err(Error::NotFound(file_id))
	}
}

async fn get_all(
	extract::State(state): extract::State<State>,
) -> Result<(StatusCode, Json<Vec<LockedNode>>), Error> {
	let nodes = state.nodes.lock().await.get_all();

	println!("returnin {} nodes", nodes.len());

	Ok((StatusCode::OK, Json(nodes)))
}

async fn purge(extract::State(mut state): extract::State<State>) -> Result<StatusCode, Error> {
	println!("purgin...");

	state.purge().await;

	clear_uploads_dir().await;

	Ok(StatusCode::OK)
}

async fn webauthn_start_reg(
	extract::State(state): extract::State<State>,
	Path(user_id): Path<Uid>,
) -> Result<(StatusCode, Json<webauthn::Registration>), Error> {
	let reg = webauthn::Registration::new();

	let mut wauth = state.webauthn.lock().await;

	wauth.add_registration(user_id, reg.clone());

	Ok((StatusCode::CREATED, Json(reg)))
}

async fn webauthn_finish_reg(
	extract::State(state): extract::State<State>,
	Path(user_id): Path<Uid>,
	extract::Json(bundle): extract::Json<webauthn::Bundle>,
) -> Result<StatusCode, Error> {
	let mut wauth = state.webauthn.lock().await;

	println!("finishing req with creds: {:?}", bundle);

	let reg = wauth
		.consume_registration(user_id)
		.ok_or(Error::Unauthorised)?;

	if webauthn::verify_reg_challenge(&bundle.cred.client_data_json, reg.challenge) {
		wauth.add_passkey(user_id, reg.prf_salt, bundle);

		Ok(StatusCode::CREATED)
	} else {
		Err(Error::Unauthorised)
	}
}

async fn webauthn_start_auth(
	extract::State(state): extract::State<State>,
) -> Result<(StatusCode, Json<webauthn::AuthChallenge>), Error> {
	let mut wauth = state.webauthn.lock().await;

	println!("statring auth");

	let ch = webauthn::AuthChallenge::new();
	wauth.add_auth_challenge(ch.clone());

	Ok((StatusCode::CREATED, Json(ch)))
}

// may also return a session id or a bearer token if for non e2e related authentication
async fn webauthn_finish_auth(
	extract::State(state): extract::State<State>,
	Path(ch_id): Path<Uid>,
	extract::Json(auth): extract::Json<webauthn::Authentication>,
) -> Result<(StatusCode, Json<webauthn::Passkey>), Error> {
	println!("finishing auth");

	let mut wauth = state.webauthn.lock().await;
	let ch = wauth
		.consume_auth_challenge(ch_id)
		.ok_or(Error::Unauthorised)?;

	if webauthn::verify_auth_challenge(&auth, ch) {
		let pk = wauth
			.passkey_for_credential_id(&auth.id)
			.ok_or(Error::Unauthorised)?
			.clone();
		Ok((StatusCode::OK, Json(pk)))
	} else {
		Err(Error::Unauthorised)
	}
}

async fn get_passkeys_for_user(
	extract::State(state): extract::State<State>,
	Path(user_id): Path<Uid>,
) -> Result<(StatusCode, Json<Vec<webauthn::Passkey>>), Error> {
	println!("getting passkeys for {}", user_id.to_base64());

	let pks = state.webauthn.lock().await.passkeys_for_user(user_id);

	Ok((StatusCode::OK, Json(pks)))
}

async fn delete_passkey(
	extract::State(state): extract::State<State>,
	Path(user_id): Path<u64>,
	Path(pk_id): Path<webauthn::CredentialId>,
) -> Result<StatusCode, Error> {
	println!("getting passkeys for {}", user_id);

	let mut wauth = state.webauthn.lock().await;
	wauth.remove_passkey(pk_id);

	Ok(StatusCode::OK)
}

async fn clear_uploads_dir() {
	_ = tokio::fs::remove_dir_all("uploads").await;
	_ = tokio::fs::create_dir("uploads").await;
}

async fn remove_file(id: Uid) {
	let path = path_for_file_id(id);

	_ = tokio::fs::remove_file(path).await;
}

fn path_for_file_id(id: Uid) -> String {
	format!("./uploads/{}", id.to_base64())
}

#[tokio::main]
async fn main() {
	clear_uploads_dir().await;

	let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
	let state = State::new();
	let use_tls = env::var("USE_TLS").unwrap_or_else(|_| "false".into()) == "true";
	let router = router(state);

	if use_tls {
		let config = RustlsConfig::from_pem_file(
			PathBuf::from("certs").join("cert.pem"),
			PathBuf::from("certs").join("key.pem"),
		)
		.await
		.unwrap();

		axum_server::bind_rustls(addr, config)
			.serve(router.into_make_service())
			.await
			.unwrap();
	} else {
		Server::bind(addr)
			.serve(router.into_make_service())
			.await
			.unwrap();
	}
}

#[allow(dead_code)]
fn router(state: State) -> Router {
	Router::new()
		.route("/uploads/stream/:file_id", post(upload_stream))
		.route("/uploads/chunk/:file_id", post(upload_ranged))
		.route("/uploads/chunk/:file_id", get(download_ranged))
		// TODO: /uploads/finish/:file_id/
		.route("/uploads/:file_id", head(check_file_length))
		.route("/nodes", post(add_nodes))
		.route("/nodes/:file_id", delete(delete_node))
		.route("/nodes", get(get_all))
		.route("/purge", post(purge))
		.route("/signup", post(signup))
		.route("/sessions/lock/:token_id", post(lock_session))
		.route("/sessions/unlock/:token_id", post(unlock_session))
		.route("/users/:user_id/mk", get(get_master_key))
		.route("/users/:user_id", get(get_user))
		.route(
			"/users/:user_id/webauthn-passkeys",
			get(get_passkeys_for_user),
		)
		.route(
			"/users/:user_id/webauthn-passkeys/:pk_id",
			delete(delete_passkey),
		)
		.route("/login", post(login))
		.route("/invite/:email", get(get_invite))
		.route("/invite", post(invite))
		.route("/webauthn/start-reg/:user_id", post(webauthn_start_reg))
		.route("/webauthn/finish-reg/:user_id", post(webauthn_finish_reg))
		.route("/webauthn/start-auth", post(webauthn_start_auth))
		.route("/webauthn/finish-auth/:id", post(webauthn_finish_auth))
		.layer(CorsLayer::permissive())
		.with_state(state)
}
