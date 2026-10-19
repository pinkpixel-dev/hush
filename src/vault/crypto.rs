use age::secrecy::SecretString;
use age::{scrypt, x25519};
use anyhow::{Context, Result, anyhow};
use zeroize::Zeroizing;

/// What unlocks the vault: an age identity from the keychain, or a passphrase.
pub enum Key {
    Identity(x25519::Identity),
    Passphrase(SecretString),
}

pub fn encrypt(key: &Key, plaintext: &[u8]) -> Result<Vec<u8>> {
    match key {
        Key::Identity(identity) => age::encrypt(&identity.to_public(), plaintext),
        Key::Passphrase(passphrase) => age::encrypt(&scrypt::Recipient::new(passphrase.clone()), plaintext),
    }
    .context("failed to encrypt the vault")
}

pub fn decrypt(key: &Key, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let plaintext = match key {
        Key::Identity(identity) => age::decrypt(identity, ciphertext),
        Key::Passphrase(passphrase) => age::decrypt(&scrypt::Identity::new(passphrase.clone()), ciphertext),
    };
    plaintext.map(Zeroizing::new).map_err(|e| match key {
        Key::Passphrase(_) => anyhow!("couldn't unlock the vault. Wrong passphrase? ({e})"),
        Key::Identity(_) => anyhow!("couldn't unlock the vault with the key in your keychain ({e})"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trip() {
        let key = Key::Identity(x25519::Identity::generate());
        let ciphertext = encrypt(&key, b"secret stuff").unwrap();
        assert!(!ciphertext.windows(6).any(|w| w == b"secret"));
        assert_eq!(decrypt(&key, &ciphertext).unwrap().as_slice(), b"secret stuff");
    }

    #[test]
    fn wrong_identity_fails() {
        let key = Key::Identity(x25519::Identity::generate());
        let other = Key::Identity(x25519::Identity::generate());
        let ciphertext = encrypt(&key, b"secret stuff").unwrap();
        assert!(decrypt(&other, &ciphertext).is_err());
    }
}
