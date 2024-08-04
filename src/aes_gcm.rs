use crate::base64_blobs::{deserialize_array_base64, serialize_array_base64};
use serde::{Deserialize, Serialize};

const KEY_SIZE: usize = 32;
const IV_SIZE: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
	#[serde(
		serialize_with = "serialize_array_base64::<_, KEY_SIZE>",
		deserialize_with = "deserialize_array_base64::<_, KEY_SIZE>"
	)]
	pub bytes: [u8; Self::SIZE],
}

impl Key {
	pub const SIZE: usize = KEY_SIZE;
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Iv {
	#[serde(
		serialize_with = "serialize_array_base64::<_, {IV_SIZE}>",
		deserialize_with = "deserialize_array_base64::<_, IV_SIZE>"
	)]
	pub bytes: [u8; Self::SIZE],
}

impl Iv {
	pub const SIZE: usize = IV_SIZE;
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Aes {
	pub key: Key,
	pub iv: Iv,
}
