use serde::{Deserialize, Serialize};

use crate::{
	base64_blobs::{deserialize_vec_base64, serialize_vec_base64},
	ed25519,
	id::Uid,
	kyber, x448,
};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Public {
	// created by by the inviting party (unless god)
	pub id: Uid,
	// can be used to encrypt messages to or verify signatures against
	pub x448: x448::PublicKeyX448,
	pub ed25519: ed25519::PublicKeyEd25519,
	pub kyber: kyber::PublicKeyKyber,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Encrypted {
	// layer 0: aes-encrypted data
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	ct: Vec<u8>,
	// layer 2: kyber encrypted ecc key (which in turn encrypts layer 1)
	ecc_ct: kyber::Encrypted,
}

impl Public {
	pub fn id(&self) -> Uid {
		// id::from_bytes(&[self.x448.as_bytes(), self.ed25519.as_bytes().as_slice()].concat())
		self.id
	}
}
