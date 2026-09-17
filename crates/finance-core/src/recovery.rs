//! Recovery key: 256 random bits shown once at setup, formatted for writing
//! down, e.g. `ABCD-EFGH-...` (13 groups of 4 base32 characters).

use data_encoding::BASE32_NOPAD;
use zeroize::Zeroizing;

use crate::crypto::{random_key, SecretKey, KEY_LEN};
use crate::error::{CoreError, Result};

/// Generate a new recovery key. Returns the raw bytes (used as the KDF input)
/// and the human-readable form.
pub fn generate() -> (SecretKey, Zeroizing<String>) {
    let raw = random_key();
    let text = format_key(&raw);
    (raw, text)
}

pub fn format_key(raw: &[u8; KEY_LEN]) -> Zeroizing<String> {
    let enc = Zeroizing::new(BASE32_NOPAD.encode(raw)); // 52 chars
    let mut out = Zeroizing::new(String::with_capacity(64));
    for (i, ch) in enc.chars().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(ch);
    }
    out
}

/// Parse user input, tolerating lowercase, spaces and dashes, and the common
/// look-alikes 0 -> O and 1 -> I (base32 uses A-Z and 2-7, so 0 and 1 never
/// appear in a real key).
pub fn parse(input: &str) -> Result<SecretKey> {
    let cleaned: Zeroizing<String> = Zeroizing::new(
        input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| match c.to_ascii_uppercase() {
                '0' => 'O',
                '1' => 'I',
                other => other,
            })
            .collect(),
    );
    let bytes = Zeroizing::new(
        BASE32_NOPAD
            .decode(cleaned.as_bytes())
            .map_err(|_| CoreError::InvalidRecoveryKey)?,
    );
    if bytes.len() != KEY_LEN {
        return Err(CoreError::InvalidRecoveryKey);
    }
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_has_13_groups() {
        let (_, text) = generate();
        assert_eq!(text.split('-').count(), 13);
        assert!(text.split('-').all(|g| g.len() == 4));
    }

    #[test]
    fn parse_round_trips_and_is_forgiving() {
        let (raw, text) = generate();
        assert_eq!(*parse(&text).unwrap(), *raw);
        let messy = format!("  {}  ", text.to_lowercase().replace('-', " "));
        assert_eq!(*parse(&messy).unwrap(), *raw);
    }

    #[test]
    fn parse_handles_every_alphabet_letter() {
        // Regression: 'L' is a valid base32 character and must not be remapped.
        for _ in 0..200 {
            let (raw, text) = generate();
            assert_eq!(*parse(&text).unwrap(), *raw, "{}", *text);
        }
    }

    #[test]
    fn parse_maps_digit_lookalikes() {
        let (raw, text) = generate();
        let swapped = text.replace('O', "0").replace('I', "1");
        assert_eq!(*parse(&swapped).unwrap(), *raw);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse("not a key").is_err());
        assert!(parse("ABCD-EFGH").is_err());
    }
}
