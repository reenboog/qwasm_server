use serde::{Deserialize, Serialize};

pub const NO_PARENT_ID: u64 = u64::MAX;
pub(crate) const ROOT_ID: u64 = 0;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct LockedNode {
	pub(crate) id: u64,
	pub(crate) parent_id: u64,
	pub(crate) content: Vec<u8>,
	pub(crate) dirty: bool,
	// pending?
}
