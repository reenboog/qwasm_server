use std::collections::HashMap;

use crate::{
	base64_blobs::{deserialize_vec_base64, serialize_vec_base64},
	encrypted,
	id::Uid,
	purge::Purge,
};

use crate::salt::Salt;
use serde::{Deserialize, Serialize};

// See https://www.w3.org/TR/webauthn-2/ for details

// just a random value; if not specified, individual salts will be generated for each passkey registration
const PRF_SALT: Option<&[u8; Salt::SIZE]> = Some(b"k47,0V=0#f6fn!yfN2Osy-ht,.%ay4md");

pub struct Webauthn {
	// { user_id, Registration }
	pending_registrations: HashMap<Uid, Registration>,
	auth_challenges: HashMap<Uid, Salt>,
	passkeys: HashMap<CredentialId, Passkey>,
}

impl Purge for Webauthn {
	fn new() -> Self {
		Self {
			pending_registrations: HashMap::new(),
			auth_challenges: HashMap::new(),
			passkeys: HashMap::new(),
		}
	}
}

impl Webauthn {
	pub fn add_registration(&mut self, id: Uid, reg: Registration) {
		self.pending_registrations.insert(id, reg);
	}

	pub fn consume_registration(&mut self, user_id: Uid) -> Option<Registration> {
		self.pending_registrations.remove(&user_id)
	}

	pub fn add_passkey(
		&mut self,
		user_id: Uid,
		prf_salt: Salt,
		bundle: Bundle,
	) {
		self.passkeys.insert(
			bundle.cred.id.clone(),
			Passkey {
				prf_salt,
				id: bundle.cred.id,
				user_id,
				name: bundle.cred.name.to_owned(),
				pub_key: bundle.cred.attestation,
				mk: bundle.mk,
			},
		);
	}

	pub fn passkeys_for_user(&self, user_id: Uid) -> Vec<Passkey> {
		self.passkeys
			.values()
			.filter(|&pk| pk.user_id == user_id)
			.cloned()
			.collect()
	}

	pub fn remove_passkey(&mut self, id: CredentialId) {
		self.passkeys.remove(&id);
	}

	pub fn passkey_for_credential_id(&self, id: &CredentialId) -> Option<&Passkey> {
		self.passkeys.get(id)
	}

	pub fn add_auth_challenge(&mut self, ch: AuthChallenge) {
		self.auth_challenges.insert(ch.id, ch.challenge);
	}

	pub fn consume_auth_challenge(&mut self, id: Uid) -> Option<Salt> {
		self.auth_challenges.remove(&id)
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Registration {
	pub challenge: Salt,
	pub prf_salt: Salt,
}

impl Registration {
	pub fn new() -> Self {
		Self {
			challenge: Salt::generate(),
			prf_salt: PRF_SALT.map_or_else(
				|| Salt::generate(),
				|bytes| Salt {
					bytes: bytes.clone(),
				},
			),
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthChallenge {
	pub id: Uid,
	pub challenge: Salt,
	pub prf_salt: Option<Salt>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Bundle {
	pub cred: Credential,
	pub mk: encrypted::Encrypted,
}

impl AuthChallenge {
	pub fn new() -> Self {
		Self {
			id: Uid::generate(),
			challenge: Salt::generate(),
			prf_salt: PRF_SALT.map_or(None, |bytes| {
				Some(Salt {
					bytes: bytes.clone(),
				})
			}),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Authentication {
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	pub id: CredentialId,
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	pub authenticator_data: Vec<u8>,
	pub client_data_json: String,
}

pub type CredentialId = Vec<u8>;
#[derive(Serialize, Deserialize, Debug)]
pub struct Credential {
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	pub id: CredentialId,
	pub name: String,
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	// public key + attestation statement + authenticator meta
	pub attestation: Vec<u8>,
	// {
	// 	"type": "webauthn.create",
	// 	"challenge": base64-encoded,
	// 	"origin": origin-url,
	// 	"crossOrigin": boolean
	// }
	pub client_data_json: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Passkey {
	pub prf_salt: Salt,
	pub user_id: Uid,
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	pub id: CredentialId,
	pub name: String,
	#[serde(
		serialize_with = "serialize_vec_base64",
		deserialize_with = "deserialize_vec_base64"
	)]
	pub pub_key: Vec<u8>,
	pub mk: encrypted::Encrypted,
}

pub fn verify_reg_challenge(_ch: &str, _against: Salt) -> bool {
	// TODO: implement
	// 1 decode ch
	// 2 extract the challenge
	// 3 assert(ch.extracted_ch == aghainst)
	// 4 extract pub_key
	// 5 verify the signature
	true
}

pub fn verify_auth_challenge(_ch: &Authentication, _against: Salt) -> bool {
	// TODO: implement
	// pub_key_by_credential_id(id).verify(ch.authenticatorData + hash(clientDataJSON))]
	true
}