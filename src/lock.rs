use serde::{Deserialize, Serialize};

use crate::{
	base64_blobs::{deserialize_vec_base64, serialize_vec_base64},
	encrypted::Encrypted,
};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Lock {
	// pt encrypted with master_key
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	pub ct: Vec<u8>,
	// master_key encrypted with pass
	pub master_key: Encrypted,
}
