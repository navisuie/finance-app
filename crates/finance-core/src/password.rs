//! Master-password policy. Kept deliberately simple: length matters most.

use crate::error::{CoreError, Result};

pub const MIN_LEN: usize = 12;

const COMMON: &[&str] = &[
    "password", "123456", "qwerty", "letmein", "iloveyou", "admin", "welcome",
    "monkey", "dragon", "football", "baseball", "master", "sunshine", "princess",
];

pub fn validate(pw: &str) -> Result<()> {
    let len = pw.chars().count();
    if len < MIN_LEN {
        return Err(CoreError::WeakPassword(format!(
            "Use at least {MIN_LEN} characters. A short phrase of 3-4 random words works well."
        )));
    }
    if len > 1024 {
        return Err(CoreError::WeakPassword("That password is too long.".into()));
    }
    let lower = pw.to_lowercase();
    if COMMON.iter().any(|c| lower.contains(c)) && len < 20 {
        return Err(CoreError::WeakPassword(
            "That password contains a very common word. Try something less guessable.".into(),
        ));
    }
    let first = pw.chars().next().unwrap();
    if pw.chars().all(|c| c == first) {
        return Err(CoreError::WeakPassword("Don't repeat a single character.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_common_and_repeated() {
        assert!(validate("short").is_err());
        assert!(validate("mypassword12").is_err());
        assert!(validate("aaaaaaaaaaaaaaaa").is_err());
    }

    #[test]
    fn accepts_passphrase() {
        assert!(validate("maple river lantern 42").is_ok());
    }
}
