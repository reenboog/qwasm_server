use std::collections::HashMap;

use crate::{
	base64_blobs::{deserialize_array_base64, serialize_array_base64},
	ed448, identity, lock,
	nodes::LockedNode,
};
use serde::{Deserialize, Serialize};

const SEED_SIZE: usize = 32;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub struct Seed {
	#[serde(
		serialize_with = "serialize_array_base64::<_, SEED_SIZE>",
		deserialize_with = "deserialize_array_base64::<_, SEED_SIZE>"
	)]
	pub(crate) bytes: [u8; SEED_SIZE],
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Export {
	// no sig is required here; validate LockedShare instead
	pub receiver: u64,
	// these are ids of the exported seeds
	pub fs: Vec<u64>,
	pub db: Vec<u64>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
// when unlocking, the backend is to return all LockedShare where id == sender.id() || export.receiver
pub struct LockedShare {
	pub sender: identity::Public,
	// ids of the share (convenient to return roots to unlock)
	pub export: Export,
	// encrypted content of the sahre
	pub payload: identity::Encrypted,
	// sign({ sender, exports })
	pub sig: ed448::Signature,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Invite {
	pub(crate) user_id: u64,
	pub(crate) sender: identity::Public,
	pub(crate) email: String,
	pub(crate) payload: lock::Lock,
	pub(crate) export: Export,
	pub(crate) sig: ed448::Signature,
}

#[derive(Serialize, Deserialize)]
pub struct Welcome {
	pub(crate) user_id: u64,
	pub(crate) sender: identity::Public,
	pub(crate) imports: lock::Lock,
	// = Invite::sig
	pub(crate) sig: ed448::Signature,
	// TODO: get_nodes(invite.export.fs.ids)
	pub(crate) nodes: Vec<LockedNode>,
}

pub struct Shares {
	pub shares: Vec<LockedShare>,
	pub invites: HashMap<String, Invite>,
}

impl Shares {
	pub fn new() -> Self {
		Self {
			shares: Vec::new(),
			invites: HashMap::new(),
		}
	}

	pub fn add_share(&mut self, share: LockedShare) {
		self.shares.push(share);
	}

	pub fn all_shares_for_user(&self, user_id: u64) -> Vec<LockedShare> {
		self.shares
			.iter()
			.filter(|&share| share.sender.id() == user_id || share.export.receiver == user_id)
			.cloned()
			.collect()
	}

	pub fn add_invite(&mut self, invite: Invite, email: &str) {
		self.invites.insert(email.to_string(), invite);
	}

	pub fn invie_for_mail(&self, email: &str) -> Option<&Invite> {
		self.invites.get(email)
	}

	pub fn delete_invite(&mut self, email: &str) {
		self.invites.remove(email);
	}
}

// get_nodes(locked_shares(user_id == share.receiver | user_id == 0 then node_id_root).export.fs.ids + children)
// { user_id, share }
// shares: HashMap<u64, LockedShare>,
