use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

use super::HASHLINE_PUBLIC_HEX_LEN;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineHash(String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineAnchor {
    line: usize,
    hash: LineHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct LineAnchorParseError {
    message: String,
}

impl LineHash {
    pub(super) fn from_content(content: &str) -> Self {
        let full_hex = blake3::hash(content.as_bytes()).to_hex();
        Self(full_hex[..HASHLINE_PUBLIC_HEX_LEN].to_string())
    }

    fn parse(anchor: &str, value: &str) -> Result<Self, LineAnchorParseError> {
        if value.len() != HASHLINE_PUBLIC_HEX_LEN {
            return Err(LineAnchorParseError {
                message: format!(
                    "Invalid hashline anchor '{}': hash must be exactly {} hex chars",
                    anchor, HASHLINE_PUBLIC_HEX_LEN
                ),
            });
        }
        if !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err(LineAnchorParseError {
                message: format!(
                    "Invalid hashline anchor '{}': hash must contain only hex characters",
                    anchor
                ),
            });
        }

        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LineHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Deref for LineHash {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Serialize for LineHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl LineAnchor {
    pub fn parse(value: &str) -> Result<Self, LineAnchorParseError> {
        let raw = value.trim();
        let without_display_suffix = raw.split_once('|').map_or(raw, |(prefix, _)| prefix).trim();
        let (line_raw, hash_raw) =
            without_display_suffix
                .split_once(':')
                .ok_or_else(|| LineAnchorParseError {
                    message: format!(
                        "Invalid hashline anchor '{}': expected format '<line>:<hex-hash>'",
                        value
                    ),
                })?;

        let line = line_raw
            .trim()
            .parse::<usize>()
            .map_err(|_| LineAnchorParseError {
                message: format!(
                    "Invalid hashline anchor '{}': line number must be a positive integer",
                    value
                ),
            })?;
        if line == 0 {
            return Err(LineAnchorParseError {
                message: format!(
                    "Invalid hashline anchor '{}': line number must be >= 1",
                    value
                ),
            });
        }

        let hash = LineHash::parse(value, hash_raw.trim())?;
        Ok(Self { line, hash })
    }

    pub fn try_new(line: usize, hash: LineHash) -> Result<Self, LineAnchorParseError> {
        if line == 0 {
            return Err(LineAnchorParseError {
                message: "Invalid hashline anchor: line number must be >= 1".to_string(),
            });
        }
        Ok(Self { line, hash })
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn hash(&self) -> &LineHash {
        &self.hash
    }
}

impl fmt::Display for LineAnchor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.line, self.hash)
    }
}

impl Serialize for LineAnchor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LineAnchor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}
