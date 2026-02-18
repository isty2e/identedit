pub const HASH_HEX_LEN: usize = 16;

pub fn hash_bytes(bytes: &[u8]) -> String {
    let full_hex = blake3::hash(bytes).to_hex().to_string();
    shorten_hex(&full_hex)
}

pub fn hash_text(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

pub fn shorten_hex(full_hex: &str) -> String {
    let prefix_len = HASH_HEX_LEN.min(full_hex.len());
    full_hex[..prefix_len].to_string()
}
