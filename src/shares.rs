use std::collections::HashMap;

use crate::{
	base64_blobs::{deserialize_array_base64, serialize_array_base64},
	ed25519,
	id::Uid,
	identity, lock,
	nodes::LockedNode,
	purge::Purge,
	users, x448,
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
	pub receiver: Uid,
	// these are ids of the exported seeds
	pub fs: Vec<Uid>,
	pub db: Vec<Uid>,
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
	pub sig: ed25519::Signature,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Invite {
	pub(crate) user_id: Uid,
	pub(crate) sender: identity::Public,
	pub(crate) email: String,
	pub(crate) payload: lock::Lock,
	pub(crate) export: Export,
	pub(crate) sig: ed25519::Signature,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum Index {
	Table { table: String },
	Column { table: String, column: String },
}

// a pin-less invite intent that should be later acknowledged
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct InviteIntent {
	pub(crate) email: String,
	pub(crate) sender: identity::Public,
	// sign(email + user_id + sender.id)
	pub(crate) sig: ed25519::Signature,
	pub(crate) user_id: Uid,
	// receiver's pk which the sender is to use to finally encrypt the previously selected seeds
	pub(crate) receiver: Option<identity::Public>,
	// None means `root`
	// do I need these at all?
	pub(crate) fs_ids: Option<Vec<Uid>>,
	pub(crate) db_ids: Option<Vec<Index>>,
}

#[derive(Serialize, Deserialize)]
pub struct FinishInviteIntent {
	pub(crate) email: String,
	pub(crate) share: LockedShare,
}

#[derive(Serialize, Deserialize)]
pub struct Welcome {
	pub(crate) user_id: Uid,
	pub(crate) sender: identity::Public,
	pub(crate) imports: lock::Lock,
	// = Invite::sig
	pub(crate) sig: ed25519::Signature,
	// TODO: get_nodes(invite.export.fs.ids)
	pub(crate) nodes: Vec<LockedNode>,
}

pub struct Shares {
	pub shares: Vec<LockedShare>,
	pub invites: HashMap<String, Invite>,
	pub intents: HashMap<String, InviteIntent>,
}

impl Shares {
	pub fn add_share(&mut self, share: LockedShare) {
		self.shares.push(share);
	}

	pub fn all_shares_for_user(&self, user_id: Uid) -> Vec<LockedShare> {
		self.shares
			.iter()
			.filter(|&share| share.sender.id() == user_id || share.export.receiver == user_id)
			.cloned()
			.collect()
	}

	pub fn add_invite(&mut self, invite: Invite) {
		self.invites.insert(invite.email.to_string(), invite);
	}

	pub fn add_invite_intent(&mut self, intent: InviteIntent) {
		self.intents.insert(intent.email.to_string(), intent);
	}

	pub fn get_invite_intent(&self, email: &str) -> Option<&InviteIntent> {
		self.intents.get(email)
	}

	pub fn get_invite_intents_for_sender(&self, sender: Uid) -> Vec<InviteIntent> {
		self.intents
			.values()
			.filter(|int| int.sender.id() == sender)
			.cloned()
			.collect()
	}

	pub fn delete_invite_intent(&mut self, email: &str) -> Option<InviteIntent> {
		self.intents.remove(email)
	}

	pub fn ack_invite_intent(&mut self, email: &str, pk: identity::Public) -> bool {
		if let Some(intent) = self.intents.get_mut(email) {
			if intent.receiver.is_none() {
				// no need to ack more than once
				intent.receiver = Some(pk);
				true
			} else {
				false
			}
		} else {
			false
		}
	}

	pub fn invie_for_mail(&self, email: &str) -> Option<&Invite> {
		self.invites.get(email)
	}

	pub fn delete_invite(&mut self, email: &str) {
		self.invites.remove(email);
	}
}

impl Purge for Shares {
	fn new() -> Self {
		Self {
			shares: Vec::new(),
			invites: HashMap::new(),
			intents: HashMap::new(),
		}
	}
}

// get_nodes(locked_shares(user_id == share.receiver | user_id == 0 then node_id_root).export.fs.ids + children)
// { user_id, share }
// shares: HashMap<u64, LockedShare>,
