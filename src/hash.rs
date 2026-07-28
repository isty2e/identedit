use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

pub const HASH_HEX_LEN: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("hash must be exactly {HASH_HEX_LEN} ASCII hex characters")]
pub struct ContentHashParseError;

impl ContentHash {
    pub fn parse(value: &str) -> Result<Self, ContentHashParseError> {
        if value.len() != HASH_HEX_LEN || !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err(ContentHashParseError);
        }

        Ok(Self(value.to_ascii_lowercase()))
    }

    fn from_blake3(hash: blake3::Hash) -> Self {
        let full_hex = hash.to_hex();
        Self(full_hex[..HASH_HEX_LEN].to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Deref for ContentHash {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

impl From<ContentHash> for String {
    fn from(hash: ContentHash) -> Self {
        hash.0
    }
}

pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    ContentHash::from_blake3(blake3::hash(bytes))
}

pub fn hash_text(text: &str) -> ContentHash {
    hash_bytes(text.as_bytes())
}

pub fn shorten_hex(full_hex: &str) -> String {
    let prefix_len = HASH_HEX_LEN.min(full_hex.len());
    full_hex[..prefix_len].to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ContentHash, HASH_HEX_LEN, hash_text};

    #[test]
    fn content_hash_normalizes_uppercase_and_round_trips_as_a_string() {
        let parsed = ContentHash::parse("ABCDEF0123456789").expect("uppercase hash should parse");
        assert_eq!(parsed.as_str(), "abcdef0123456789");

        let serialized = serde_json::to_value(&parsed).expect("hash should serialize");
        assert_eq!(serialized, json!("abcdef0123456789"));
        let reparsed: ContentHash =
            serde_json::from_value(serialized).expect("hash should deserialize");
        assert_eq!(reparsed, parsed);
    }

    #[test]
    fn content_hash_rejects_invalid_lengths_non_hex_and_unicode() {
        for value in [
            "",
            "0123456789abcde",
            "0123456789abcdef0",
            "0123456789abcdeg",
            "éééééééé",
        ] {
            assert!(
                ContentHash::parse(value).is_err(),
                "invalid hash should fail: {value:?}"
            );
        }
    }

    #[test]
    fn generated_content_hash_satisfies_the_canonical_contract_for_unicode_bytes() {
        let hash = hash_text("café\n한글\n");
        assert_eq!(hash.len(), HASH_HEX_LEN);
        assert!(hash.as_bytes().iter().all(u8::is_ascii_hexdigit));
        assert_eq!(hash, hash_text("café\n한글\n"));
    }
}
