use crate::base64_blobs::{deserialize_array_base64, serialize_array_base64};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

const SALT_SIZE: usize = 32;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Salt {
	#[serde(
		serialize_with = "serialize_array_base64::<_, SALT_SIZE>",
		deserialize_with = "deserialize_array_base64::<_, SALT_SIZE>"
	)]
	pub bytes: [u8; Self::SIZE],
}

impl Salt {
	pub const SIZE: usize = SALT_SIZE;

	pub fn generate() -> Self {
		let mut bytes = [0u8; Self::SIZE];
		OsRng.fill_bytes(&mut bytes);

		Self { bytes }
	}
}
