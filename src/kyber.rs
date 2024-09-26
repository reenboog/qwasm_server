use serde::{Deserialize, Serialize};

use crate::public_key::PublicKey;

#[derive(Debug, PartialEq)]
pub struct KeyTypeKyber;

const PUB_KEY_SIZE: usize = 1568;
const CT_SIZE: usize = 1568;
pub type PublicKeyKyber = PublicKey<KeyTypeKyber, { PUB_KEY_SIZE }>;
pub type CiphertextKyber = PublicKey<KeyTypeKyber, { CT_SIZE }>;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Encrypted {
	kyber_ct: CiphertextKyber,
	ct: Vec<u8>,
}
