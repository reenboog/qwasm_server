use crate::{id::Uid, purge::Purge};
use aws_sdk_s3::{
	self,
	config::{BehaviorVersion, Credentials, Region},
	presigning::PresigningConfig,
	types::{CompletedMultipartUpload, CompletedPart},
	Config,
};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// TODO: if needed, add more algorithms
const ALG_AES_GCM: &str = "aes-gcm";
const PRESIGNED_URL_EXPIRY: u64 = 10 * 60;

#[derive(Debug)]
pub enum Error {
	GenUploadId(String),
	GenPresignedUrls(String),
	DeleteFile(String),
	CompleteUpload(String),
	GetStatus(String, String),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Error::GenUploadId(msg) => write!(f, "Failed to generate upload ID: {}", msg),
			Error::GenPresignedUrls(msg) => write!(f, "Failed to generate presigned URLs: {}", msg),
			Error::DeleteFile(msg) => write!(f, "Failed to delete file {}", msg),
			Error::CompleteUpload(file_id) => write!(f, "Failed to complete upload {}", file_id),
			Error::GetStatus(file_id, msg) => {
				write!(f, "Failed to get upload status {}, {}", file_id, msg)
			}
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct S3Part {
	part_number: i32,
	e_tag: String,
}

impl From<aws_sdk_s3::types::CompletedPart> for S3Part {
	fn from(part: aws_sdk_s3::types::CompletedPart) -> Self {
		S3Part {
			part_number: part.part_number.unwrap(),
			e_tag: part.e_tag.unwrap(),
		}
	}
}

impl From<aws_sdk_s3::types::Part> for S3Part {
	fn from(part: aws_sdk_s3::types::Part) -> Self {
		S3Part {
			part_number: part.part_number.unwrap(),
			e_tag: part.e_tag.unwrap(),
		}
	}
}

impl From<S3Part> for aws_sdk_s3::types::CompletedPart {
	fn from(part: S3Part) -> Self {
		aws_sdk_s3::types::CompletedPart::builder()
			.part_number(part.part_number)
			.e_tag(part.e_tag)
			.build()
	}
}

#[derive(Serialize, Deserialize)]
pub struct NewUploadRes {
	pub id: String,
	pub chunk_urls: Vec<String>,
	pub chunk_size: i64,
	pub enc_alg: String,
}

#[derive(Serialize, Deserialize)]
pub struct NewUploadReq {
	pub file_size: i64,
}

#[derive(Serialize, Deserialize)]
pub enum UploadStatus {
	Pending { parts: Vec<S3Part> },
	Ready { url: String, content_length: i64 },
}

#[derive(Serialize, Deserialize)]
pub struct UploadInfo {
	pub status: UploadStatus,
	pub enc_alg: String,
	pub chunk_size: i64,
}

#[derive(Clone)]
pub struct Upload {
	pub enc_alg: String,
	pub upload_id: String,
	pub chunk_size: i64,
	pub complete: bool,
}

pub struct Uploads {
	uploads: HashMap<Uid, Upload>,
}

impl Purge for Uploads {
	fn new() -> Self {
		Uploads {
			uploads: HashMap::new(),
		}
	}

	fn purge(&mut self) {
		self.uploads.clear();
	}
}

impl Uploads {
	// an algorithm could be selected at an earlier stage, but for now just pick one and return it
	pub fn add(&mut self, file_id: Uid, upload_id: String, chunk_size: i64) -> String {
		self.uploads.insert(
			file_id,
			Upload {
				enc_alg: ALG_AES_GCM.to_string(),
				upload_id,
				chunk_size,
				complete: false,
			},
		);

		ALG_AES_GCM.to_string()
	}

	pub fn get(&self, file_id: Uid) -> Option<&Upload> {
		self.uploads.get(&file_id)
	}

	pub fn remove(&mut self, file_ids: &[Uid]) -> Vec<Uid> {
		let mut deleted = Vec::new();

		file_ids.into_iter().for_each(|&id| {
			if self.uploads.remove(&id).is_some() {
				deleted.push(id);
			}
		});

		deleted
	}

	pub fn mark_as_complete(&mut self, file_id: Uid) -> bool {
		if let Some(upload) = self.uploads.get_mut(&file_id) {
			upload.complete = true;
			true
		} else {
			false
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct FinishUpload {
	pub upload_id: String,
	pub parts: Vec<S3Part>,
}

#[derive(Debug)]
pub struct PartitionPlan {
	pub chunk_size: i64,
	pub num_chunks: usize,
}

pub fn partition_file(file_size: i64) -> PartitionPlan {
	let chunk_size = if file_size < 50 * 1024 * 1024 {
		5 * 1024 * 1024
	} else if file_size < 500 * 1024 * 1024 {
		10 * 1024 * 1024
	} else {
		50 * 1024 * 1024
	};

	let num_chunks = ((file_size + chunk_size - 1) / chunk_size) as usize;

	PartitionPlan {
		chunk_size,
		num_chunks,
	}
}

/*

AWS S3 Endpoints: AWS S3 endpoints are region-specific. For example, the endpoint for the us-east-1 region is s3.us-east-1.amazonaws.com. You may need to allow traffic to the specific S3 endpoint for the region you are using. Here are some common S3 endpoints:
s3.amazonaws.com (global endpoint)
s3.us-east-1.amazonaws.com (US East (N. Virginia))
s3.eu-west-1.amazonaws.com (EU (Ireland))

*/

pub fn s3_config(ak_id: &str, ak_secret: &str, region: &str, accelerate: bool) -> Config {
	Config::builder()
		.region(Region::new(region.to_string()))
		.accelerate(accelerate)
		.credentials_provider(Credentials::new(ak_id, ak_secret, None, None, "static"))
		.behavior_version(BehaviorVersion::latest())
		.build()
}

pub async fn s3_get_upload_status(
	client: &aws_sdk_s3::Client,
	bucket: &str,
	file_id: &Uid,
	upload: &Upload,
) -> Result<UploadStatus, Error> {
	let key = file_id.to_base64();
	let status = if upload.complete {
		let presigning_config = PresigningConfig::builder()
			.expires_in(std::time::Duration::from_secs(PRESIGNED_URL_EXPIRY))
			.build()
			.map_err(|e| Error::GetStatus(key.clone(), e.to_string()))?;
		let res = client
			.get_object()
			.bucket(bucket)
			.key(key.clone())
			.presigned(presigning_config)
			.await
			.map_err(|e| Error::GetStatus(key.clone(), e.to_string()))?;
		let content_length = client
			.head_object()
			.bucket(bucket)
			.key(key.clone())
			.send()
			.await
			.map_err(|e| Error::GetStatus(key.clone(), e.to_string()))?
			.content_length()
			.unwrap_or(0);

		println!(
			"upload complete, url: {}, content_length: {}",
			res.uri().to_string(),
			content_length
		);

		UploadStatus::Ready {
			url: res.uri().to_string(),
			content_length,
		}
	} else {
		let parts = client
			.list_parts()
			.bucket(bucket)
			.key(key.clone())
			.upload_id(upload.upload_id.clone())
			.send()
			.await
			.map_err(|e| Error::GetStatus(key, e.to_string()))?;

		println!("upload incomplete, parts: {:?}", parts);

		UploadStatus::Pending {
			parts: parts
				.parts()
				.into_iter()
				.map(|p| p.clone().into())
				.collect(),
		}
	};

	Ok(status)
}

pub async fn s3_finish_upload(
	client: &aws_sdk_s3::Client,
	bucket: &str,
	file_id: &Uid,
	upload_id: &str,
	parts: Vec<S3Part>,
) -> Result<(), Error> {
	let mut parts: Vec<CompletedPart> = parts.into_iter().map(|p| p.into()).collect();
	parts.sort_by_key(|part| part.part_number);

	let completed_upload = CompletedMultipartUpload::builder()
		.set_parts(Some(parts))
		.build();
	client
		.complete_multipart_upload()
		.bucket(bucket)
		.key(&file_id.to_base64())
		.upload_id(upload_id)
		.multipart_upload(completed_upload)
		.send()
		.await
		.map_err(|_| Error::CompleteUpload(file_id.to_base64()))?;

	Ok(())
}

pub async fn s3_delete_uploads(
	client: &aws_sdk_s3::Client,
	bucket: &str,
	file_ids: &Vec<Uid>,
) -> Result<(), Error> {
	// s3 imposes a limit of 1000 objects per request, so split this request into smaller tasks, if need be
	let chunk_size = 1000;
	let mut tasks = Vec::new();

	for chunk in file_ids.chunks(chunk_size) {
		if let Ok(delete) = aws_sdk_s3::types::Delete::builder()
			.set_objects(Some(
				chunk
					.iter()
					.filter_map(|file_id| {
						aws_sdk_s3::types::ObjectIdentifier::builder()
							.key(file_id.to_base64())
							.build()
							.ok()
					})
					.collect(),
			))
			.build()
		{
			let client = client.clone();
			let bucket = bucket.to_string();

			tasks.push(tokio::spawn(async move {
				client
					.delete_objects()
					.bucket(bucket)
					.delete(delete)
					.send()
					.await
					.map_err(|e| Error::DeleteFile(e.to_string()))?;

				Ok::<(), Error>(())
			}));
		}
	}

	let _ = try_join_all(tasks)
		.await
		.map_err(|e| Error::DeleteFile(e.to_string()))?
		.into_iter()
		.collect::<Result<Vec<_>, _>>()?;

	Ok(())
}

pub async fn s3_gen_upload_id(
	client: &aws_sdk_s3::Client,
	bucket: &str,
	file_id: &Uid,
) -> Result<String, Error> {
	let key = file_id.to_base64();
	let resp = client
		.create_multipart_upload()
		.bucket(bucket)
		.key(key)
		.send()
		.await
		.map_err(|e| Error::GenUploadId(e.to_string()))?;

	Ok(resp.upload_id.unwrap().to_string())
}

// TODO: to continue an interrupted upload, do:
// 1 /uploads/info/:upload_id to get remaining part_numbers, if any
// 2 generate presigned urls for the remaining part_numbers
// 3 (client side) read and upload the file chunks for each part_number
pub async fn s3_gen_presigned_urls(
	client: &aws_sdk_s3::Client,
	bucket: &str,
	file_id: &Uid,
	upload_id: &str,
	num_parts: usize,
) -> Result<Vec<String>, Error> {
	// TODO: expiry should be based on the size of the file
	let presigning_config = aws_sdk_s3::presigning::PresigningConfig::builder()
		.expires_in(std::time::Duration::from_secs(PRESIGNED_URL_EXPIRY))
		.build()
		.map_err(|e| Error::GenPresignedUrls(e.to_string()))?;

	let mut tasks = Vec::new();

	for part_number in 1..=num_parts {
		let client = client.clone();
		let bucket = bucket.to_string();
		let key = file_id.to_base64();
		let upload_id = upload_id.to_string();
		let presigning_config = presigning_config.clone();

		tasks.push(tokio::spawn(async move {
			let presigned_request = client
				.upload_part()
				.bucket(bucket)
				.key(key)
				.upload_id(upload_id)
				.part_number(part_number as i32)
				.presigned(presigning_config)
				.await
				.map_err(|e| Error::GenPresignedUrls(e.to_string()))?;

			Ok(presigned_request.uri().to_string())
		}));
	}

	let results = try_join_all(tasks)
		.await
		.map_err(|e| Error::GenPresignedUrls(e.to_string()))?;
	let urls: Result<Vec<String>, Error> = results.into_iter().collect();

	urls
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_partition_file_small_chunk() {
		let file_size = 4 * 1024 * 1024; // 4 MB
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 5 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 1);
	}

	#[test]
	fn test_partition_file_zero_size() {
		let file_size = 0;
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 5 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 0);
	}

	#[test]
	fn test_partition_file_6mb() {
		let file_size = 6 * 1024 * 1024;
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 5 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 2);
	}

	#[test]
	fn test_partition_file_7mb() {
		let file_size = 7 * 1024 * 1024;
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 5 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 2);
	}

	#[test]
	fn test_partition_file_11mb() {
		let file_size = 11 * 1024 * 1024;
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 5 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 3);
	}

	#[test]
	fn test_partition_file_medium_chunk() {
		let file_size = 211 * 1024 * 1024;
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 10 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 22);
	}

	#[test]
	fn test_partition_file_large_chunk() {
		let file_size = 501 * 1024 * 1024;
		let partition = partition_file(file_size);
		assert_eq!(partition.chunk_size, 50 * 1024 * 1024);
		assert_eq!(partition.num_chunks, 11);
	}
}
