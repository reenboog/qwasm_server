use std::collections::HashMap;

use crate::{
	encrypted,
	id::Uid,
	identity, lock,
	nodes::LockedNode,
	purge::Purge,
	shares::{InviteIntent, LockedShare},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct LockedUser {
	// password-encrypted identity::Private
	// aes_encrypted?
	pub encrypted_priv: lock::Lock,
	#[serde(rename = "pub")]
	pub _pub: identity::Public,
	// exports & imports will be decoded from this; god has empty imports, always
	// sent, ackend and encrypted shared
	pub shares: Vec<LockedShare>,
	// sent and optionally acked shares (could be useful to cancel, if not yet accepted)
	pub pending_invite_intents: Vec<InviteIntent>,
	// get_nodes(locked_shares(user_id == share.receiver | user_id == 0 then node_id_root).export.fs.ids + children)
	// TODO: include a hash of the hierarchy for later checks
	pub roots: Vec<LockedNode>,
}

#[derive(Serialize, Deserialize)]
pub struct Signup {
	pub email: String,
	pub pass: String,
	pub user: LockedUser,
}

#[derive(Serialize, Deserialize)]
pub struct Login {
	pub email: String,
	pub pass: String,
}

pub struct Users {
	// no pass is needed here, since it's just a playground
	// { email, user_id }
	pub credentials: HashMap<String, Uid>,
	// { user_id, Public }
	pub public_keys: HashMap<Uid, identity::Public>,
	// { user_id, Lock }
	pub private_keys: HashMap<Uid, lock::Lock>,
}

impl Users {
	pub fn add_priv(&mut self, id: Uid, _priv: lock::Lock) {
		self.private_keys.insert(id, _priv);
	}

	pub fn priv_for_id(&self, user_id: Uid) -> Option<&lock::Lock> {
		self.private_keys.get(&user_id)
	}

	pub fn add_pub(&mut self, id: Uid, _pub: identity::Public) {
		self.public_keys.insert(id, _pub);
	}

	pub fn pub_for_id(&self, user_id: Uid) -> Option<&identity::Public> {
		self.public_keys.get(&user_id)
	}

	pub fn mk_for_id(&self, user_id: Uid) -> Option<&encrypted::Encrypted> {
		self.priv_for_id(user_id).map(|p| &p.master_key)
	}

	pub fn add_credentials(&mut self, email: &str, id: Uid) {
		self.credentials.insert(email.to_string(), id);
	}

	pub fn id_for_email(&self, email: &str) -> Option<Uid> {
		self.credentials.get(email).cloned()
	}
}

impl Purge for Users {
	fn new() -> Self {
		Self {
			credentials: HashMap::new(),
			public_keys: HashMap::new(),
			private_keys: HashMap::new(),
		}
	}
}
// invites
// users:
// 	priv
//  pub
