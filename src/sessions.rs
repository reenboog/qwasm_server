use crate::{purge::Purge, shares::Seed};
use std::collections::HashMap;

pub struct Sessions {
	// { token_id, token }
	pub tokens: HashMap<String, Seed>,
	// webauth_ids
}

impl Sessions {
	pub fn add_token(&mut self, id: &str, token: Seed) {
		self.tokens.insert(id.to_string(), token);
	}

	pub fn consume_token_by_id(&mut self, id: &str) -> Option<Seed> {
		self.tokens.remove(id)
	}
}

impl Purge for Sessions {
	fn new() -> Self {
		Self {
			tokens: HashMap::new(),
		}
	}
}
