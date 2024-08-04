use std::collections::HashMap;

use crate::{identity, lock, nodes::LockedNode, shares::LockedShare};
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
	pub credentials: HashMap<String, u64>,
	// { user_id, Public }
	pub public_keys: HashMap<u64, identity::Public>,
	// { user_id, Lock }
	pub private_keys: HashMap<u64, lock::Lock>,
}

impl Users {
	pub fn new() -> Self {
		Self {
			credentials: HashMap::new(),
			public_keys: HashMap::new(),
			private_keys: HashMap::new(),
		}
	}

	pub fn add_priv(&mut self, id: u64, _priv: lock::Lock) {
		self.private_keys.insert(id, _priv);
	}

	pub fn priv_for_id(&self, user_id: u64) -> Option<&lock::Lock> {
		self.private_keys.get(&user_id)
	}

	pub fn add_pub(&mut self, id: u64, _pub: identity::Public) {
		self.public_keys.insert(id, _pub);
	}

	pub fn pub_for_id(&self, user_id: u64) -> Option<&identity::Public> {
		self.public_keys.get(&user_id)
	}

	pub fn add_credentials(&mut self, email: &str, id: u64) {
		self.credentials.insert(email.to_string(), id);
	}

	pub fn id_for_email(&self, email: &str) -> Option<u64> {
		self.credentials.get(email).cloned()
	}
}

// invites
// users:
// 	priv
//  pub
