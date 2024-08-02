pub mod content_range;
pub mod node;
pub mod storage;

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
use node::LockedNode;
use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};
use storage::Storage;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::{fs::OpenOptions, sync::Mutex};

// Define a custom error type that can convert into an HTTP response
#[derive(Debug)]
enum Error {
	IOError(std::io::Error),
	ReadError(hyper::Error),
	Unauthorised,
	InvalidRange,
	NotFound(u64),
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		Error::IOError(err)
	}
}

impl From<hyper::Error> for Error {
	fn from(err: hyper::Error) -> Self {
		Error::ReadError(err)
	}
}

impl IntoResponse for Error {
	fn into_response(self) -> Response {
		let status = match self {
			Error::IOError(_) => StatusCode::SERVICE_UNAVAILABLE,
			Error::ReadError(_) => StatusCode::SERVICE_UNAVAILABLE,
			Error::Unauthorised => StatusCode::FORBIDDEN,
			Error::InvalidRange => StatusCode::RANGE_NOT_SATISFIABLE,
			Error::NotFound(_) => StatusCode::NOT_FOUND,
		};

		status.into_response()
	}
}

#[derive(Clone)]
struct State {
	storage: Arc<Mutex<Storage>>,
}

impl State {
	fn new() -> Self {
		Self {
			storage: Arc::new(Mutex::new(Storage::new())),
		}
	}
}

async fn open_file_at_offset(
	file_id: &str,
	create: bool,
	append: bool,
	write: bool,
	offset: u64,
) -> Result<tokio::fs::File, Error> {
	let path = format!("./uploads/{}", file_id);

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
	file_id: &str,
	mut file: tokio::fs::File,
	mut stream: BodyDataStream,
) -> Result<StatusCode, Error> {
	println!("auth passed, working...");

	while let Some(chunk) = stream.next().await {
		let data = chunk.unwrap(); // FIXME: handle errors properly
		println!("{}: chunk size - {}", file_id, data.len());

		file.write_all(&data).await?;
	}

	Ok(StatusCode::OK)
}

async fn handle_upload(
	Path(file_id): Path<String>,
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

	let file = open_file_at_offset(&file_id, true, append, true, range.range_start).await?;
	let stream = request.into_body().into_data_stream();

	process_data_stream(&file_id, file, stream).await
}

async fn upload_stream(
	Path(file_id): Path<String>,
	request: Request<Body>,
) -> Result<StatusCode, Error> {
	handle_upload(Path(file_id), request, false).await
}

async fn upload_ranged(
	Path(file_id): Path<String>,
	request: Request<Body>,
) -> Result<StatusCode, Error> {
	handle_upload(Path(file_id), request, false).await
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
		None => Err(Error::IOError(std::io::Error::new(
			std::io::ErrorKind::NotFound,
			"File not found",
		))),
	}
}

async fn add_nodes(
	extract::State(state): extract::State<State>,
	extract::Json(nodes): extract::Json<Vec<LockedNode>>,
) -> Result<StatusCode, Error> {
	let mut storage = state.storage.lock().await;

	nodes.into_iter().for_each(|n| {
		println!("inserting {}", n.id);

		storage.add(n);
	});

	Ok(StatusCode::CREATED)
}

async fn delete_node(
	extract::State(state): extract::State<State>,
	Path(file_id): Path<u64>,
) -> Result<StatusCode, Error> {
	if let Some(_) = state.storage.lock().await.remove(file_id) {
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
	let nodes = state.storage.lock().await.get_all();

	println!("returnin {} nodes", nodes.len());

	Ok((StatusCode::OK, Json(nodes)))
}

async fn purge(extract::State(state): extract::State<State>) -> Result<StatusCode, Error> {
	println!("purgin all nodes");

	state.storage.lock().await.purge();

	Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() {
	// Remove and recreate the uploads directory for testing
	let _ = tokio::fs::remove_dir_all("uploads").await;
	let _ = tokio::fs::create_dir("uploads").await;

	// Define the address and port to bind the server
	let addr = SocketAddr::from(([0, 0, 0, 0], 3000));

	// Create the application state
	let state = State::new();

	// Check the environment variable to determine if TLS should be used
	let use_tls = env::var("USE_TLS").unwrap_or_else(|_| "false".into()) == "true";
	let router = router(state);

	if use_tls {
		// Load the TLS configuration
		let config = RustlsConfig::from_pem_file(
			PathBuf::from("certs").join("cert.pem"),
			PathBuf::from("certs").join("key.pem"),
		)
		.await
		.unwrap();

		// Bind and serve the application with TLS
		axum_server::bind_rustls(addr, config)
			.serve(router.into_make_service())
			.await
			.unwrap();
	} else {
		// Bind and serve the application without TLS
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
		.route("/nodes/purge", post(purge))
		.with_state(state)
}
