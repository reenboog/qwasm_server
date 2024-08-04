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
mod salt;
mod shares;
mod users;
mod x448;

use axum::{
	body::{Body, BodyDataStream},
	extract::{self, Path, Request},
	http::{HeaderMap, HeaderValue, Response as HttpResponse, StatusCode},
	response::{IntoResponse, Response},
	routing::{delete, get, head, post},
	Json, Router,
};
use axum_server::{tls_rustls::RustlsConfig, Server};
use content_range::ContentRange;
use futures_util::StreamExt;
use nodes::LockedNode;
use nodes::Nodes;
use shares::{Shares, Welcome};
use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::{fs::OpenOptions, sync::Mutex};
use users::{LockedUser, Login, Signup, Users};

// Define a custom error type that can convert into an HTTP response
#[derive(Debug)]
enum Error {
	Io(String),
	Unauthorised,
	InvalidRange,
	NotFound(u64),
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
}

impl State {
	fn new() -> Self {
		Self {
			nodes: Arc::new(Mutex::new(Nodes::new())),
			shares: Arc::new(Mutex::new(Shares::new())),
			users: Arc::new(Mutex::new(Users::new())),
		}
	}

	fn purge(&mut self) {
		*self = Self::new();
	}
}

async fn open_file_at_offset(
	file_id: u64,
	create: bool,
	append: bool,
	write: bool,
	offset: u64,
) -> Result<tokio::fs::File, Error> {
	let path = path_for_file_id(file_id);

	println!("path to process: {}", path);

	let mut file = OpenOptions::new()
		.create(create)
		.append(append)
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
	file_id: u64,
	mut file: tokio::fs::File,
	mut stream: BodyDataStream,
) -> Result<StatusCode, Error> {
	println!("auth passed, working...");

	while let Some(chunk) = stream.next().await {
		let data = chunk?;

		file.write_all(&data).await?;
		println!("{}: chunk size - {}", file_id, data.len());
	}

	Ok(StatusCode::OK)
}

async fn handle_upload(
	file_id: u64,
	request: Request<Body>,
	append: bool,
) -> Result<StatusCode, Error> {
	let range = request
		.headers()
		.get("Content-Range")
		.and_then(|header| header.to_str().ok())
		.and_then(|header_str| ContentRange::from_str(header_str).ok())
		.ok_or(Error::InvalidRange)?;

	check_auth(&request.headers()).await?;

	println!(
		"received: {}-{}/{} ",
		range.range_start,
		range.range_end,
		range.size.unwrap_or(0)
	);

	let file = open_file_at_offset(file_id, true, append, true, range.range_start).await?;
	let stream = request.into_body().into_data_stream();

	process_data_stream(file_id, file, stream).await
}

async fn upload_stream(
	Path(file_id): Path<u64>,
	request: Request<Body>,
) -> Result<StatusCode, Error> {
	handle_upload(file_id, request, false).await
}

async fn upload_ranged(
	Path(file_id): Path<u64>,
	request: Request<Body>,
) -> Result<StatusCode, Error> {
	handle_upload(file_id, request, false).await
}

async fn file_length(file_path: String) -> Option<usize> {
	if let Ok(file) = OpenOptions::new().read(true).open(file_path).await {
		if let Ok(metadata) = file.metadata().await {
			return Some(metadata.len() as usize);
		}
	}
	None
}

async fn check_file_length(Path(file_id): Path<String>) -> Result<HttpResponse<Body>, Error> {
	let file_path = format!("uploads/{}", file_id);

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
		println!("inserting {}", n.id);

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
		shares.add(share.clone());
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
	let nodes = state.nodes.lock().await;
	let shares = state.shares.lock().await;
	let users = state.users.lock().await;

	// not sure, if it's unauthorised (more likely) or not found
	let user_id = users
		.id_for_email(&login.email)
		.ok_or(Error::Unauthorised)?;
	let _priv = users.priv_for_id(user_id).ok_or(Error::Unauthorised)?;
	let _pub = users.pub_for_id(user_id).ok_or(Error::Unauthorised)?;
	let shares = shares.all_shares_for_user(user_id);
	// FIXME: return nodes based on exports and pending uploads (if uploader == users.id_for_email(email))
	let roots = nodes.get_all();

	println!("logged in {}", login.email);

	// you'd generate an access token here for subsequent requests

	Ok((
		StatusCode::OK,
		Json(LockedUser {
			encrypted_priv: _priv.clone(),
			_pub: _pub.clone(),
			shares,
			roots,
		}),
	))
}

async fn get_invite(
	extract::State(state): extract::State<State>,
	Path(email): Path<String>,
) -> Result<(StatusCode, Json<Welcome>), Error> {
	let shares = state.shares.lock().await;
	let nodes = state.nodes.lock().await;

	if let Some(invite) = shares.invie_for_mail(&email) {
		let welcome = Welcome {
			user_id: invite.user_id,
			sender: invite.sender.clone(),
			imports: invite.payload.clone(),
			sig: invite.sig.clone(),
			// FIXME: return nodes based on exports and pending uploads (if uploader == users.id_for_email(email))
			nodes: nodes.get_all(),
		};

		Ok((StatusCode::CREATED, Json(welcome)))
	} else {
		Err(Error::NoInvite(email))
	}
}

async fn delete_node(
	extract::State(state): extract::State<State>,
	Path(file_id): Path<u64>,
) -> Result<StatusCode, Error> {
	if let Some(_) = state.nodes.lock().await.remove(file_id) {
		remove_file(file_id).await;

		println!("deleted {}", file_id);

		Ok(StatusCode::NO_CONTENT)
	} else {
		println!("can not delete {}; not found", file_id);

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

	state.purge();

	clear_uploads_dir().await;

	Ok(StatusCode::OK)
}

async fn clear_uploads_dir() {
	_ = tokio::fs::remove_dir_all("uploads").await;
	_ = tokio::fs::create_dir("uploads").await;
}

async fn remove_file(id: u64) {
	let path = path_for_file_id(id);

	_ = tokio::fs::remove_file(path).await;
}

fn path_for_file_id(id: u64) -> String {
	format!("./uploads/{}", id)
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
		.route("/uploads/:file_id", head(check_file_length))
		.route("/nodes", post(add_nodes))
		.route("/nodes/:file_id", delete(delete_node))
		.route("/nodes", get(get_all))
		.route("/purge", post(purge))
		.route("/signup", post(signup))
		.route("/login", post(login))
		.route("/invite/:email", get(get_invite))
		.with_state(state)
}
