mod aes_gcm;
mod base64_blobs;
mod ed25519;
mod encrypted;
mod id;
mod identity;
mod key;
mod lock;
mod nodes;
mod public_key;
mod purge;
mod s3;
mod salt;
mod sessions;
mod shares;
mod users;
mod webauthn;
mod x448;

use crate::purge::Purge;
use aws_sdk_s3::{
	config::{BehaviorVersion, Credentials, Region},
	presigning::PresigningConfig,
	types::{CompletedMultipartUpload, CompletedPart},
	Client, Config,
};
use axum::{
	extract::{self, Path},
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::{delete, get, post},
	Json, Router,
};
use axum_server::{tls_rustls::RustlsConfig, Server};

use id::Uid;
use nodes::LockedNode;
use nodes::Nodes;
use s3::Uploads;
use sessions::Sessions;
use shares::{Invite, InviteIntent, Shares, Welcome};
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use users::{LockedUser, Login, Signup, Users};
use webauthn::Webauthn;

#[derive(Debug)]
enum Error {
	Io(String),
	Unauthorised,
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

	s3_client: Arc<Mutex<Client>>,
	s3_bucket: String,

	uploads: Arc<Mutex<Uploads>>,
}

impl State {
	fn new(s3_config: Config, s3_bucket: &str) -> Self {
		Self {
			nodes: Arc::new(Mutex::new(Nodes::new())),
			shares: Arc::new(Mutex::new(Shares::new())),
			users: Arc::new(Mutex::new(Users::new())),
			sessions: Arc::new(Mutex::new(Sessions::new())),
			webauthn: Arc::new(Mutex::new(Webauthn::new())),
			s3_client: Arc::new(Mutex::new(Client::from_conf(s3_config))),
			s3_bucket: s3_bucket.to_string(),
			uploads: Arc::new(Mutex::new(Uploads::new())),
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
		{
			self.uploads.lock().await.purge();
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
		let invite_intents = shares.get_invite_intents_for_sender(id);
		let shares = shares.all_shares_for_user(id);
		// FIXME: return nodes based on exports and pending uploads (if uploader == users.id_for_email(email))
		let roots = nodes.get_all();

		Ok(LockedUser {
			encrypted_priv: _priv.clone(),
			_pub: _pub.clone(),
			shares,
			roots,
			pending_invite_intents: invite_intents,
		})
	}
}

async fn get_upload_status(
	extract::State(state): extract::State<State>,
	Path(file_id): Path<Uid>,
) -> Result<(StatusCode, Json<s3::UploadInfo>), Error> {
	let upload = state
		.uploads
		.lock()
		.await
		.get(file_id)
		.ok_or(Error::NotFound(file_id))?
		.clone();
	let client = &state.s3_client.lock().await;
	let status = if upload.complete {
		let presigning_config = PresigningConfig::builder()
			.expires_in(std::time::Duration::from_secs(10 * 60))
			.build()
			.map_err(|e| Error::Io(e.to_string()))?;
		let res = client
			.get_object()
			.bucket(state.s3_bucket.clone())
			.key(file_id.to_base64())
			.presigned(presigning_config)
			.await
			.map_err(|e| Error::Io(e.to_string()))?;
		let content_length = client
			.head_object()
			.bucket(state.s3_bucket)
			.key(file_id.to_base64())
			.send()
			.await
			.map_err(|e| Error::Io(e.to_string()))?
			.content_length()
			.unwrap_or(0);

		println!(
			"upload complete, url: {}, content_length: {}",
			res.uri().to_string(),
			content_length
		);

		s3::UploadStatus::Ready {
			url: res.uri().to_string(),
			content_length,
		}
	} else {
		let parts = client
			.list_parts()
			.bucket(state.s3_bucket)
			.key(file_id.to_base64())
			.upload_id(upload.upload_id.clone())
			.send()
			.await
			.map_err(|e| Error::Io(e.to_string()))?;

		println!("upload incomplete, parts: {:?}", parts);

		s3::UploadStatus::Pending {
			parts: parts
				.parts()
				.into_iter()
				.map(|p| p.clone().into())
				.collect(),
		}
	};

	let info = s3::UploadInfo {
		status,
		enc_alg: upload.enc_alg,
		chunk_size: upload.chunk_size,
	};

	Ok((StatusCode::OK, Json(info)))
}

async fn start_upload(
	extract::State(state): extract::State<State>,
	Path(file_id): Path<Uid>,
	extract::Json(req): extract::Json<s3::NewUploadReq>,
) -> Result<(StatusCode, Json<s3::NewUploadRes>), Error> {
	println!(
		"starting upload: {}; size: {}",
		file_id.to_base64(),
		req.file_size
	);

	let file_name = file_id.to_base64();
	let bucket = state.s3_bucket;
	let plan = s3::partition_file(req.file_size);
	let client = &state.s3_client.lock().await;
	let upload_id = s3::s3_gen_upload_id(client, &bucket, &file_name)
		.await
		.map_err(|e| {
			println!("error generating upload id: {}", e.to_string());
			Error::Io(e.to_string())
		})?;

	println!("upload id: {}", upload_id);
	println!("partitions plan: {:?}", plan);

	let presigned_urls =
		s3::s3_gen_presigned_urls(client, &bucket, &file_name, &upload_id, plan.num_chunks)
			.await
			.map_err(|e| Error::Io(e.to_string()))?;

	println!("presigned urls: {:?}", presigned_urls);

	let enc_alg = state
		.uploads
		.lock()
		.await
		.add(file_id, upload_id.clone(), plan.chunk_size);

	let new_upload = s3::NewUploadRes {
		id: upload_id,
		chunk_urls: presigned_urls,
		chunk_size: plan.chunk_size,
		enc_alg,
	};

	Ok((StatusCode::CREATED, Json(new_upload)))
}

async fn finish_upload(
	extract::State(state): extract::State<State>,
	Path(file_id): Path<Uid>,
	extract::Json(payload): extract::Json<s3::FinishUpload>,
) -> Result<StatusCode, Error> {
	let file_name = file_id.to_base64();
	let client = &state.s3_client.lock().await;
	let bucket = &state.s3_bucket;
	let mut parts: Vec<CompletedPart> = payload.parts.into_iter().map(|p| p.into()).collect();
	parts.sort_by_key(|part| part.part_number);

	let completed_upload = CompletedMultipartUpload::builder()
		.set_parts(Some(parts))
		.build();

	println!(
		"completing upload: {}; upload_id: {}",
		file_id.to_base64(),
		payload.upload_id
	);

	client
		.complete_multipart_upload()
		.bucket(bucket)
		.key(&file_name)
		.upload_id(&payload.upload_id)
		.multipart_upload(completed_upload)
		.send()
		.await
		.map_err(|e| {
			println!("error completing upload: {}", e.to_string());
			Error::Io(e.to_string())
		})?;

	state.uploads.lock().await.mark_as_complete(file_id);

	println!("upload completed: {}", file_id.to_base64());

	Ok(StatusCode::OK)
}

// async fn check_auth(headers: &HeaderMap) -> Result<(), Error> {
// 	if headers
// 		.get("x-uploader-auth")
// 		.eq(&Some(&HeaderValue::from_bytes(b"aabb1122").unwrap()))
// 	{
// 		Ok(())
// 	} else {
// 		Err(Error::Unauthorised)
// 	}
// }

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

	println!(
		"ack invite intent for: {}; res: {}",
		signup.email,
		shares.ack_invite_intent(&signup.email, user._pub.clone())
	);

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

async fn start_invite_intent(
	extract::State(state): extract::State<State>,
	extract::Json(intent): extract::Json<InviteIntent>,
) -> Result<StatusCode, Error> {
	let mut shares = state.shares.lock().await;
	let email = intent.email.clone();

	println!("start invite intent: {}", email);

	shares.add_invite_intent(intent);

	println!("added intent");

	Ok(StatusCode::CREATED)
}

async fn get_invite_intent(
	extract::State(state): extract::State<State>,
	Path(email_base64): Path<String>,
) -> Result<(StatusCode, Json<InviteIntent>), Error> {
	let email = String::from_utf8(
		base64::decode_config(&email_base64, base64::URL_SAFE)
			.map_err(|_| Error::NoInvite(email_base64.clone()))?,
	)
	.map_err(|_| Error::NoInvite(email_base64.clone()))?;

	let shares = state.shares.lock().await;
	println!("getting intent for: {}", email);

	if let Some(intent) = shares.get_invite_intent(&email) {
		println!("--intent for: {}; res: {:?}", email, intent,);

		Ok((StatusCode::OK, Json(intent.clone())))
	} else {
		Err(Error::NoInvite(email.to_string()))
	}
}

async fn finish_invite_intents_if_any(
	extract::State(state): extract::State<State>,
	extract::Json(intents): extract::Json<Vec<shares::FinishInviteIntent>>,
) -> Result<StatusCode, Error> {
	let mut shares = state.shares.lock().await;

	println!("finish invite intent");

	intents.iter().for_each(|int| {
		shares.add_share(int.share.clone());

		let email = &int.email;

		println!(
			"--intent: {}; res: {:?}",
			email,
			shares.delete_invite_intent(email)
		);
	});

	Ok(StatusCode::OK)
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
	let use_tls = env::var("USE_TLS").unwrap_or_else(|_| "false".into()) == "true";
	let s3_ak_id = env::var("S3_AK_ID").expect("S3_AK_ID not set");
	let s3_ak = env::var("S3_AK_SECRET").expect("S3_AK_SECRET not set");
	let s3_bucket = env::var("S3_BUCKET").expect("S3_BUCKET not set");
	let s3_region = env::var("S3_REGION").expect("S3_REGION not set");
	let state = State::new(s3_config(&s3_ak_id, &s3_ak, &s3_region, false), &s3_bucket);
	let router = router(state);

	println!("starting...");

	println!("s3_ak_id: {}", s3_ak_id);
	println!("s3_ak: {}", s3_ak);
	println!("s3_bucket: {}", s3_bucket);
	println!("s3_region: {}", s3_region);

	if use_tls {
		println!("using tls");

		let domain = env::var("DOMAIN").unwrap();
		let base_dir = PathBuf::from("/etc/letsencrypt/live/").join(&domain);
		let config = RustlsConfig::from_pem_file(
			base_dir.join("fullchain.pem"),
			base_dir.join("privkey.pem"),
		)
		.await
		.unwrap();

		println!("certs found...");

		axum_server::bind_rustls(addr, config)
			.serve(router.into_make_service())
			.await
			.unwrap();
	} else {
		println!("not using tls");

		Server::bind(addr)
			.serve(router.into_make_service())
			.await
			.unwrap();
	}
}

fn s3_config(ak_id: &str, ak_secret: &str, region: &str, accelerate: bool) -> Config {
	Config::builder()
		.region(Region::new(region.to_string()))
		.accelerate(accelerate)
		.credentials_provider(Credentials::new(ak_id, ak_secret, None, None, "static"))
		.behavior_version(BehaviorVersion::latest())
		.build()
}

#[allow(dead_code)]
fn router(state: State) -> Router {
	Router::new()
		.route("/uploads/info/:file_id", get(get_upload_status))
		.route("/uploads/start/:file_id", post(start_upload))
		.route("/uploads/finish/:file_id", post(finish_upload))
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
		.route("/invite/pinned/:email", get(get_invite))
		.route("/invite/pinned", post(invite))
		// TODO: an api for share?
		.route("/invite/intent/start", post(start_invite_intent))
		.route("/invite/intent/fetch/:email", get(get_invite_intent))
		.route("/invite/intent/finish/", post(finish_invite_intents_if_any))
		.route("/webauthn/start-reg/:user_id", post(webauthn_start_reg))
		.route("/webauthn/finish-reg/:user_id", post(webauthn_finish_reg))
		.route("/webauthn/start-auth", post(webauthn_start_auth))
		.route("/webauthn/finish-auth/:id", post(webauthn_finish_auth))
		.layer(CorsLayer::permissive())
		.with_state(state)
}
