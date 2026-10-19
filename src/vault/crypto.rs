use std::io::Write;

use age::secrecy::SecretString;
use age::{scrypt, x25519};
use anyhow::{Context, Result, anyhow, bail};
use zeroize::Zeroizing;

/// What unlocks the vault: an age identity from the keychain, or a passphrase.
pub enum Key {
    Identity(x25519::Identity),
    Passphrase(SecretString),
}

/// Encrypts to the key, plus the recovery key when there is one. age can't mix
/// a passphrase with other recipients, so passphrase vaults never have one.
pub fn encrypt(key: &Key, recovery: Option<&x25519::Recipient>, plaintext: &[u8]) -> Result<Vec<u8>> {
    match (key, recovery) {
        (Key::Identity(identity), None) => age::encrypt(&identity.to_public(), plaintext),
        (Key::Identity(identity), Some(recovery)) => encrypt_to_many(&[&identity.to_public(), recovery], plaintext),
        (Key::Passphrase(_), Some(_)) => bail!("passphrase vaults can't have a recovery key"),
        (Key::Passphrase(passphrase), None) => age::encrypt(&scrypt::Recipient::new(passphrase.clone()), plaintext),
    }
    .context("failed to encrypt the vault")
}

fn encrypt_to_many(recipients: &[&dyn age::Recipient], plaintext: &[u8]) -> Result<Vec<u8>, age::EncryptError> {
    let encryptor = age::Encryptor::with_recipients(recipients.iter().copied())?;
    let mut ciphertext = Vec::with_capacity(plaintext.len());
    let mut writer = encryptor.wrap_output(&mut ciphertext)?;
    writer.write_all(plaintext)?;
    writer.finish()?;
    Ok(ciphertext)
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
        let ciphertext = encrypt(&key, None, b"secret stuff").unwrap();
        assert!(!ciphertext.windows(6).any(|w| w == b"secret"));
        assert_eq!(decrypt(&key, &ciphertext).unwrap().as_slice(), b"secret stuff");
    }

    #[test]
    fn wrong_identity_fails() {
        let key = Key::Identity(x25519::Identity::generate());
        let other = Key::Identity(x25519::Identity::generate());
        let ciphertext = encrypt(&key, None, b"secret stuff").unwrap();
        assert!(decrypt(&other, &ciphertext).is_err());
    }

    #[test]
    fn recovery_key_also_decrypts() {
        let main = x25519::Identity::generate();
        let recovery = x25519::Identity::generate();
        let ciphertext = encrypt(&Key::Identity(main.clone()), Some(&recovery.to_public()), b"secret stuff").unwrap();
        assert_eq!(decrypt(&Key::Identity(main), &ciphertext).unwrap().as_slice(), b"secret stuff");
        assert_eq!(decrypt(&Key::Identity(recovery), &ciphertext).unwrap().as_slice(), b"secret stuff");
    }

    #[test]
    fn passphrase_rejects_a_recovery_key() {
        let key = Key::Passphrase("pw".to_string().into());
        assert!(encrypt(&key, Some(&x25519::Identity::generate().to_public()), b"x").is_err());
    }
}
