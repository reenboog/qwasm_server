use sha2::{Digest, Sha256};

pub fn from_bytes(bytes: &[u8]) -> u64 {
	u64::from_be_bytes(Sha256::digest(bytes).to_vec()[..8].try_into().unwrap())
}
