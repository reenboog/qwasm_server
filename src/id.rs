use rand::{rngs::OsRng, Rng};
use sha2::{Digest, Sha256};

pub fn from_bytes(bytes: &[u8]) -> u64 {
	u64::from_be_bytes(Sha256::digest(bytes).to_vec()[..8].try_into().unwrap())
}

pub fn as_bytes(id: u64) -> Vec<u8> {
	id.to_be_bytes().to_vec()
}

pub fn generate() -> u64 {
	let mut rng = OsRng;
	rng.gen()
}
