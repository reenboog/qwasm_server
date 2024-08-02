use serde::{Deserialize, Serialize};

use crate::{ed448, identity};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Export {
	// no sig is required here; validate LockedShare instead
	pub receiver: u64,
	// these are ids of the exported seeds
	pub fs: Vec<u64>,
	pub db: Vec<u64>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
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
