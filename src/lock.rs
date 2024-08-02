use serde::{Deserialize, Serialize};

use crate::encrypted::Encrypted;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Lock {
	// pt encrypted with master_key
	pub ct: Vec<u8>,
	// master_key encrypted with pass
	pub master_key: Encrypted,
}
