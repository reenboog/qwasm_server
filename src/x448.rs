use crate::public_key::PublicKey;

#[derive(Debug, PartialEq)]
pub struct KeyTypeX448;

const PUB_KEY_SIZE: usize = 56;
pub type PublicKeyX448 = PublicKey<KeyTypeX448, { PUB_KEY_SIZE }>;
