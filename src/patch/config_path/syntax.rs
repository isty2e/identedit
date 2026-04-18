use crate::error::IdenteditError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum PathToken {
    Key(String),
    Index(usize),
}

pub(super) fn parse_config_path(raw_path: &str) -> Result<Vec<PathToken>, IdenteditError> {
    let path = raw_path.trim();
    if path.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path cannot be empty".to_string(),
        });
    }

    let bytes = path.as_bytes();
    let mut index = 0usize;
    let mut tokens = Vec::new();

    while index < bytes.len() {
        match bytes[index] {
            b'[' => {
                let (value, consumed) = parse_index_segment(path, index)?;
                tokens.push(PathToken::Index(value));
                index = consumed;
            }
            b'.' => {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Invalid config path '{path}': unexpected '.' at byte offset {index}"
                    ),
                });
            }
            _ => {
                let start = index;
                while index < bytes.len() && is_key_char(bytes[index]) {
                    index += 1;
                }
                if start == index {
                    return Err(IdenteditError::InvalidRequest {
                        message: format!(
                            "Invalid config path '{path}': unsupported character '{}' at byte offset {index}",
                            bytes[index] as char
                        ),
                    });
                }
                tokens.push(PathToken::Key(path[start..index].to_string()));
            }
        }

        while index < bytes.len() && bytes[index] == b'[' {
            let (value, consumed) = parse_index_segment(path, index)?;
            tokens.push(PathToken::Index(value));
            index = consumed;
        }

        if index < bytes.len() {
            if bytes[index] != b'.' {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Invalid config path '{path}': expected '.' or '[' at byte offset {index}"
                    ),
                });
            }
            index += 1;
            if index >= bytes.len() {
                return Err(IdenteditError::InvalidRequest {
                    message: format!("Invalid config path '{path}': trailing '.' is not allowed"),
                });
            }
        }
    }

    if tokens.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path cannot be empty".to_string(),
        });
    }

    Ok(tokens)
}

pub(super) fn expected_path_container_error(
    raw_path: &str,
    token: &PathToken,
    actual_kind: &str,
) -> IdenteditError {
    let expected = match token {
        PathToken::Key(_) => "mapping/object",
        PathToken::Index(_) => "sequence/array",
    };

    IdenteditError::InvalidRequest {
        message: format!(
            "Config path '{raw_path}' expected {expected} at segment {}, found node kind '{actual_kind}'",
            token_display(token)
        ),
    }
}

pub(super) fn token_display(token: &PathToken) -> String {
    match token {
        PathToken::Key(key) => format!("'{key}'"),
        PathToken::Index(index) => format!("[{index}]"),
    }
}

pub(super) fn path_tokens_display(path: &[PathToken]) -> String {
    let mut output = String::new();
    for token in path {
        match token {
            PathToken::Key(key) => {
                if !output.is_empty() {
                    output.push('.');
                }
                output.push_str(key);
            }
            PathToken::Index(index) => {
                output.push_str(&format!("[{index}]"));
            }
        }
    }
    output
}

fn parse_index_segment(path: &str, start: usize) -> Result<(usize, usize), IdenteditError> {
    let bytes = path.as_bytes();
    let mut cursor = start + 1;
    let digit_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }

    if digit_start == cursor {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': expected digits after '[' at byte offset {start}"
            ),
        });
    }

    if cursor >= bytes.len() || bytes[cursor] != b']' {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': missing closing ']' for index starting at byte offset {start}"
            ),
        });
    }

    let value =
        path[digit_start..cursor]
            .parse::<usize>()
            .map_err(|_| IdenteditError::InvalidRequest {
                message: format!(
                    "Invalid config path '{path}': index '{}' is out of range",
                    &path[digit_start..cursor]
                ),
            })?;

    Ok((value, cursor + 1))
}

fn is_key_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}
