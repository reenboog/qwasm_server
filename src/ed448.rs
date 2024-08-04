use crate::{
	base64_blobs::{deserialize_array_base64, serialize_array_base64},
	public_key::PublicKey,
};
use serde::{Deserialize, Serialize};

const SIG_SIZE: usize = 114;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
	#[serde(
		serialize_with = "serialize_array_base64::<_, SIG_SIZE>",
		deserialize_with = "deserialize_array_base64::<_, SIG_SIZE>"
	)]
	bytes: [u8; Self::SIZE],
}

impl Signature {
	const SIZE: usize = SIG_SIZE;
}

#[derive(Debug, PartialEq)]
pub struct KeyTypeEd448;

const PUB_KEY_SIZE: usize = 57;
pub type PublicKeyEd448 = PublicKey<KeyTypeEd448, { PUB_KEY_SIZE }>;
