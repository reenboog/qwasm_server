use axum::{
	body::Body,
	extract::{BodyStream, Path},
	http::{Response as HttpResponse, StatusCode},
	response::{IntoResponse, Response},
	routing::{head, post},
	Router,
};
use futures_util::StreamExt;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

// Define a custom error type that can convert into an HTTP response
#[derive(Debug)]
enum FileError {
	IOError(std::io::Error),
	ReadError(hyper::Error),
}

impl From<std::io::Error> for FileError {
	fn from(err: std::io::Error) -> Self {
		FileError::IOError(err)
	}
}

impl From<hyper::Error> for FileError {
	fn from(err: hyper::Error) -> Self {
		FileError::ReadError(err)
	}
}

impl IntoResponse for FileError {
	fn into_response(self) -> Response {
		println!("error");

		match self {
			FileError::IOError(e) => println!("io error: {:?}", e),
			FileError::ReadError(e) => println!("read error: {:?}", e),
		}
		StatusCode::NOT_ACCEPTABLE.into_response()
	}
}

async fn append_data(
	Path(file_id): Path<String>,
	mut stream: BodyStream,
) -> Result<StatusCode, FileError> {
	let path = format!("./uploads/{}", file_id);

	println!("path to process: {}", path);

	let mut file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.await?;

	while let Some(chunk) = stream.next().await {
		let data = chunk.unwrap(); // FIXME: handle errors properlyc
		println!("{}: chunk size - {}", file_id, data.len());

		file.write_all(&data).await?;
	}

	Ok(StatusCode::OK)
}

async fn file_length(file_path: String) -> Option<usize> {
	if let Ok(file) = OpenOptions::new().read(true).open(file_path).await {
		if let Ok(metadata) = file.metadata().await {
			return Some(metadata.len() as usize);
		}
	}
	None
}

async fn check_file_length(Path(file_id): Path<String>) -> Result<HttpResponse<Body>, FileError> {
	let file_path = format!("uploads/{}", file_id);
	match file_length(file_path).await {
		Some(length) => Ok(Response::builder()
			.status(StatusCode::OK)
			.header("Content-Length", length.to_string())
			.body(Body::empty())
			.unwrap()),
		None => Err(FileError::IOError(std::io::Error::new(
			std::io::ErrorKind::NotFound,
			"File not found",
		))),
	}
}

#[tokio::main]
async fn main() {
	_ = tokio::fs::remove_dir_all("uploads").await;
	_ = tokio::fs::create_dir("uploads").await;

	// Setup the Axum router with the routes
	let app = Router::new()
		.route("/upload/:file_id", post(append_data))
		.route("/length/:file_id", head(check_file_length));

	// Define the address to run the server on
	let addr = "0.0.0.0:3000".parse().unwrap();

	// Print out the address to the console, so we know where the server is running
	println!("Listening on {}", addr);

	// Run the Axum server
	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}
