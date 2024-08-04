// Accepts a type name, outputs a generic key type, eg PrivateKey<T, SIZE>, PublicKey<T, SIZE>, etc
macro_rules! key {
	($type: ident) => {
		#[derive(Debug, PartialEq)]
		pub struct $type<T, const SIZE: usize> {
			bytes: [u8; SIZE],
			_marker: std::marker::PhantomData<T>,
		}

		impl<T, const SIZE: usize> $type<T, SIZE> {
			// TODO: rename to `from_bytes`?
			pub fn new(bytes: [u8; SIZE]) -> Self {
				Self {
					bytes,
					_marker: std::marker::PhantomData,
				}
			}
		}

		impl<T, const SIZE: usize> Clone for $type<T, SIZE> {
			fn clone(&self) -> Self {
				Self::new(self.bytes.clone())
			}
		}

		impl<T, const SIZE: usize> serde::Serialize for $type<T, SIZE> {
			fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
				serializer.serialize_str(&base64::encode(self.bytes))
			}
		}

		impl<'de, T, const SIZE: usize> serde::Deserialize<'de> for $type<T, SIZE> {
			fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
			where
				D: serde::Deserializer<'de>,
			{
				struct Visitor<T, const SIZE: usize>(std::marker::PhantomData<T>);

				impl<'de, T, const SIZE: usize> serde::de::Visitor<'de> for Visitor<T, SIZE> {
					type Value = $type<T, SIZE>;

					fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
						formatter.write_str("a base64 encoded string")
					}

					fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
					where
						E: serde::de::Error,
					{
						let bytes = base64::decode(v).map_err(E::custom)?;
						let bytes: [u8; SIZE] = bytes.as_slice().try_into().map_err(E::custom)?;

						Ok($type::new(bytes))
					}
				}

				deserializer.deserialize_str(Visitor(std::marker::PhantomData))
			}
		}
	};
}

pub(crate) use key;
