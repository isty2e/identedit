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
                let (token, consumed) = parse_bracket_segment(path, index)?;
                tokens.push(token);
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
            let (token, consumed) = parse_bracket_segment(path, index)?;
            tokens.push(token);
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
        PathToken::Key(key) if is_bare_key(key) => format!("'{key}'"),
        PathToken::Key(key) => format!("[{}]", json_quote_key(key)),
        PathToken::Index(index) => format!("[{index}]"),
    }
}

pub(super) fn path_tokens_display(path: &[PathToken]) -> String {
    let mut output = String::new();
    for token in path {
        match token {
            PathToken::Key(key) => {
                if is_bare_key(key) {
                    if !output.is_empty() {
                        output.push('.');
                    }
                    output.push_str(key);
                } else {
                    output.push('[');
                    output.push_str(&json_quote_key(key));
                    output.push(']');
                }
            }
            PathToken::Index(index) => {
                output.push_str(&format!("[{index}]"));
            }
        }
    }
    output
}

fn parse_bracket_segment(path: &str, start: usize) -> Result<(PathToken, usize), IdenteditError> {
    let bytes = path.as_bytes();
    let Some(next) = bytes.get(start + 1) else {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': expected numeric index or JSON string key after '[' at byte offset {start}"
            ),
        });
    };

    match *next {
        b'"' => parse_quoted_key_segment(path, start),
        b'0'..=b'9' => {
            let (index, consumed) = parse_index_segment(path, start)?;
            Ok((PathToken::Index(index), consumed))
        }
        _ => Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': expected numeric index or JSON string key after '[' at byte offset {start}"
            ),
        }),
    }
}

fn parse_quoted_key_segment(
    path: &str,
    start: usize,
) -> Result<(PathToken, usize), IdenteditError> {
    let bytes = path.as_bytes();
    let quote_start = start + 1;
    let mut cursor = quote_start + 1;
    let mut escaped = false;

    while cursor < bytes.len() {
        match (escaped, bytes[cursor]) {
            (true, _) => {
                escaped = false;
                cursor += 1;
            }
            (false, b'\\') => {
                escaped = true;
                cursor += 1;
            }
            (false, b'"') => break,
            (false, _) => cursor += 1,
        }
    }

    if cursor >= bytes.len() || bytes[cursor] != b'"' {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': missing closing JSON string quote for key starting at byte offset {start}"
            ),
        });
    }

    let key_literal = &path[quote_start..=cursor];
    let key = serde_json::from_str::<String>(key_literal).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': invalid JSON string key {key_literal}: {error}"
            ),
        }
    })?;

    let closing_bracket = cursor + 1;
    if closing_bracket >= bytes.len() || bytes[closing_bracket] != b']' {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': missing closing ']' for quoted key starting at byte offset {start}"
            ),
        });
    }

    Ok((PathToken::Key(key), closing_bracket + 1))
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

fn is_bare_key(value: &str) -> bool {
    !value.is_empty() && value.as_bytes().iter().all(|byte| is_key_char(*byte))
}

fn json_quote_key(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string to JSON should not fail")
}
