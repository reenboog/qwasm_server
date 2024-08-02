use crate::{identity, lock, node::LockedNode, share::LockedShare};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct LockedUser {
	// password-encrypted identity::Private
	// aes_encrypted?
	pub encrypted_priv: lock::Lock,
	#[serde(rename = "pub")]
	pub _pub: identity::Public,
	// exports & imports will be decoded from this; god has empty imports, always
	pub shares: Vec<LockedShare>,
	// get_nodes(locked_shares(user_id == share.receiver | user_id == 0 then node_id_root).export.fs.ids + children)
	// TODO: include a hash of the hierarchy for later checks
	pub roots: Vec<LockedNode>,
}
