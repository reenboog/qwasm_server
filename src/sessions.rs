use crate::{id::Uid, purge::Purge, shares::Seed};
use std::collections::HashMap;

pub struct Sessions {
	// { token_id, token }
	pub tokens: HashMap<Uid, Seed>,
}

impl Sessions {
	pub fn add_token(&mut self, id: Uid, token: Seed) {
		self.tokens.insert(id, token);
	}

	pub fn consume_token_by_id(&mut self, id: Uid) -> Option<Seed> {
		self.tokens.remove(&id)
	}
}

impl Purge for Sessions {
	fn new() -> Self {
		Self {
			tokens: HashMap::new(),
		}
	}
}
