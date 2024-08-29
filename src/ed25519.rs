use crate::{
	base64_blobs::{deserialize_array_base64, serialize_array_base64},
	public_key::PublicKey,
};
use serde::{Deserialize, Serialize};

const SIG_SIZE: usize = 64;

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
pub struct KeyTypeEd25519;

const PUB_KEY_SIZE: usize = 32;
pub type PublicKeyEd25519 = PublicKey<KeyTypeEd25519, { PUB_KEY_SIZE }>;
