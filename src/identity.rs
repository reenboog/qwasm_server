use serde::{Deserialize, Serialize};

use crate::{
	base64_blobs::{deserialize_vec_base64, serialize_vec_base64},
	ed448::PublicKeyEd448,
	x448::PublicKeyX448,
};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Public {
	// created by by the inviting party (unless god)
	pub id: u64,
	// can be used to encrypt messages to or verify signatures against
	pub x448: PublicKeyX448,
	pub ed448: PublicKeyEd448,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Encrypted {
	// encrypted message
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	ct: Vec<u8>,
	// an ephemeral key, dh-ed with an identity pub key
	eph_x448: PublicKeyX448,
}

impl Public {
	pub fn id(&self) -> u64 {
		// id::from_bytes(&[self.x448.as_bytes(), self.ed448.as_bytes().as_slice()].concat())
		self.id
	}
}
