use serde::{Deserialize, Serialize};

use crate::encrypted::Encrypted;

pub const NO_PARENT_ID: u64 = u64::MAX;
pub const ROOT_ID: u64 = 0;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct LockedNode {
	pub id: u64,
	pub parent_id: u64,
	pub content: Encrypted,
	pub dirty: bool,
	// pending?
}
