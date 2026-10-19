//! Sealing a connector's vendor tokens ([connectors 4]): AES-256-GCM under `CONNECTOR_KEY`, a
//! random 96-bit nonce per seal, and the connector's id as the associated data, so a sealed value
//! opens only on the connector it was sealed for. The output, stored in `connector.secret` (and
//! the pending connect's PKCE verifier), is `nonce (12 bytes) || ciphertext || tag (16 bytes)`.
//!
//! It lives in `api` because everything that seals or unseals runs there: the connect routes
//! (E3.3, E3.4), the `connectors` scheduler task that syncs (E4.1) and start-up seeding (E4.6).
//! The `FarmSource` adapters in `integrations` get a token already unsealed ([`VendorToken`]) and
//! never see the key, and `domain` holds only the opaque [`Sealed`] bytes, so neither depends on
//! a cipher.
//!
//! [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
//! [`VendorToken`]: domain::ports::VendorToken

use std::{fmt, sync::Arc};

use aes_gcm::{
    Aes256Gcm, Key, KeyInit, Nonce,
    aead::{AeadInOut, Generate},
};
use secrecy::{SecretSlice, zeroize::Zeroize};

use domain::{id::ConnectorId, ports::Sealed};

/// The nonce's length: 96 bits, GCM's native size.
const NONCE_LEN: usize = 12;
/// The tag's length: 128 bits.
const TAG_LEN: usize = 16;

/// `CONNECTOR_KEY`: 32 bytes, written as 64 hex characters (`openssl rand -hex 32`, the way every
/// other secret is generated, [infra 6]). Holds the expanded AES-256 key, which is wiped on drop;
/// `Debug` never prints it.
///
/// [infra 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/infra/README.md#6-secrets-and-configuration
#[derive(Clone)]
pub struct ConnectorKey(Arc<Aes256Gcm>);

impl fmt::Debug for ConnectorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConnectorKey([REDACTED])")
    }
}

/// Why `CONNECTOR_KEY` was refused. The message never quotes the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// Not hexadecimal.
    #[error("expected 64 hex characters (openssl rand -hex 32)")]
    NotHex,
    /// Hexadecimal, but not 32 bytes.
    #[error("expected 32 bytes (64 hex characters), got {0} bytes")]
    Length(usize),
}

/// Why a sealed value didn't open. Deliberately opaque: AES-GCM can't tell a wrong key, a wrong
/// connector and a changed byte apart, and saying more would help no one but an attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnsealError {
    /// Shorter than a nonce and a tag: not something [`ConnectorKey::seal`] wrote.
    #[error("the sealed value is too short")]
    TooShort,
    /// The tag didn't verify: another key, another connector, or a changed byte.
    #[error("the sealed value doesn't open under this key for this connector")]
    Rejected,
}

impl ConnectorKey {
    /// Parses the variable's value: 64 hex characters, either case, surrounding space ignored.
    pub fn from_hex(value: &str) -> Result<Self, KeyError> {
        let mut bytes = data_encoding::HEXLOWER_PERMISSIVE
            .decode(value.trim().as_bytes())
            .map_err(|_| KeyError::NotHex)?;
        let key = <[u8; 32]>::try_from(bytes.as_slice()).map(Self::from_bytes);
        let len = bytes.len();
        bytes.zeroize();
        key.map_err(|_| KeyError::Length(len))
    }

    /// A key from its 32 bytes. The caller wipes its own copy.
    pub fn from_bytes(mut bytes: [u8; 32]) -> Self {
        let mut key = Key::<Aes256Gcm>::from(bytes);
        let cipher = Aes256Gcm::new(&key);
        key.zeroize();
        bytes.zeroize();
        Self(Arc::new(cipher))
    }

    /// Seals `plaintext` for `connector`, with a fresh random nonce, so sealing the same token
    /// twice gives two different values.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "sealed by the code exchange (E3.4) and seeding (E4.6)"
        )
    )]
    pub fn seal(&self, connector: ConnectorId, plaintext: &[u8]) -> Sealed {
        let nonce = Nonce::generate();
        let mut body = Vec::with_capacity(plaintext.len() + TAG_LEN);
        body.extend_from_slice(plaintext);
        self.0
            .encrypt_in_place(&nonce, associated_data(connector).as_bytes(), &mut body)
            .expect(
                "AES-GCM refuses only a message over 64 GiB, and a token is a few hundred bytes",
            );
        let mut sealed = Vec::with_capacity(NONCE_LEN + body.len());
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&body);
        Sealed(sealed.into())
    }

    /// Opens a value sealed for `connector`. The plaintext comes back as a secret, wiped when
    /// dropped.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "opened by the routes (E3.4) and the sync (E4.1)")
    )]
    pub fn unseal(
        &self,
        connector: ConnectorId,
        sealed: &Sealed,
    ) -> Result<SecretSlice<u8>, UnsealError> {
        if sealed.0.len() < NONCE_LEN + TAG_LEN {
            return Err(UnsealError::TooShort);
        }
        let (nonce, body) = sealed.0.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce).map_err(|_| UnsealError::TooShort)?;
        let mut body = body.to_vec();
        let opened =
            self.0
                .decrypt_in_place(&nonce, associated_data(connector).as_bytes(), &mut body);
        if opened.is_err() {
            body.zeroize();
            return Err(UnsealError::Rejected);
        }
        Ok(SecretSlice::from(body))
    }
}

/// The associated data: the connector's id as it is written on the wire (its ULID).
fn associated_data(connector: ConnectorId) -> String {
    connector.to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use secrecy::ExposeSecret;
    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const TOKEN: &[u8] = b"xoxb-1234-5678-the-bot-token";

    fn key() -> ConnectorKey {
        ConnectorKey::from_hex(KEY_HEX).unwrap()
    }

    fn connector(n: u8) -> ConnectorId {
        format!("01JAX3K9T6R2B8V0QZ4M7D1E5{}", char::from(b'A' + n))
            .parse()
            .unwrap()
    }

    #[test]
    fn a_sealed_token_opens_for_its_connector() {
        let sealed = key().seal(connector(0), TOKEN);
        assert_eq!(sealed.0.len(), NONCE_LEN + TOKEN.len() + TAG_LEN);
        let opened = key().unseal(connector(0), &sealed).unwrap();
        assert_eq!(opened.expose_secret(), TOKEN);
        let empty = key().seal(connector(0), b"");
        assert_eq!(
            key().unseal(connector(0), &empty).unwrap().expose_secret(),
            b""
        );
    }

    #[test]
    fn a_sealed_token_never_opens_for_another_connector() {
        let sealed = key().seal(connector(0), TOKEN);
        assert_eq!(
            key().unseal(connector(1), &sealed).unwrap_err(),
            UnsealError::Rejected
        );
    }

    #[test]
    fn a_sealed_token_never_opens_under_another_key() {
        let sealed = key().seal(connector(0), TOKEN);
        let other = ConnectorKey::from_bytes([7; 32]);
        assert_eq!(
            other.unseal(connector(0), &sealed).unwrap_err(),
            UnsealError::Rejected
        );
    }

    #[test]
    fn any_changed_byte_is_refused() {
        let sealed = key().seal(connector(0), TOKEN);
        for at in 0..sealed.0.len() {
            let mut bytes = sealed.0.to_vec();
            bytes[at] ^= 0x01;
            assert_eq!(
                key()
                    .unseal(connector(0), &Sealed(bytes.into()))
                    .unwrap_err(),
                UnsealError::Rejected,
                "byte {at}"
            );
        }
        let cut = Sealed(sealed.0.slice(..NONCE_LEN + TAG_LEN - 1));
        assert_eq!(
            key().unseal(connector(0), &cut).unwrap_err(),
            UnsealError::TooShort
        );
    }

    #[test]
    fn two_seals_of_one_token_differ() {
        let first = key().seal(connector(0), TOKEN);
        let second = key().seal(connector(0), TOKEN);
        assert_ne!(first.0[..NONCE_LEN], second.0[..NONCE_LEN]);
        assert_ne!(first, second);
    }

    #[test]
    fn the_key_must_be_32_bytes_of_hex() {
        assert!(ConnectorKey::from_hex(&KEY_HEX.to_uppercase()).is_ok());
        assert!(ConnectorKey::from_hex(&format!("  {KEY_HEX}\n")).is_ok());
        assert_eq!(
            ConnectorKey::from_hex(&KEY_HEX[..62]).unwrap_err(),
            KeyError::Length(31)
        );
        assert_eq!(
            ConnectorKey::from_hex(&format!("{KEY_HEX}00")).unwrap_err(),
            KeyError::Length(33)
        );
        assert_eq!(
            ConnectorKey::from_hex(&"zz".repeat(32)).unwrap_err(),
            KeyError::NotHex
        );
        // base64 of 32 bytes is refused rather than read as something else
        assert_eq!(
            ConnectorKey::from_hex("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=").unwrap_err(),
            KeyError::NotHex
        );
    }

    /// Collects everything a subscriber writes.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn neither_the_key_nor_a_token_reaches_a_log_line() {
        let key = key();
        let sealed = key.seal(connector(0), TOKEN);
        let opened = key.unseal(connector(0), &sealed).unwrap();
        let error = key.unseal(connector(1), &sealed).unwrap_err();
        let logs = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(logs.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(?key, ?sealed, ?opened, %error, "connector token");
        });
        let line = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert!(line.contains("connector token"), "{line}");
        assert!(line.contains("ConnectorKey([REDACTED])"), "{line}");
        assert!(
            line.contains(&format!("Sealed({} bytes)", sealed.0.len())),
            "{line}"
        );
        let token = std::str::from_utf8(TOKEN).unwrap();
        for secret in [token, KEY_HEX, &KEY_HEX.to_uppercase()] {
            assert!(!line.contains(secret), "{secret} in {line}");
        }
        let sealed_hex = data_encoding::HEXLOWER.encode(&sealed.0);
        assert!(!line.contains(&sealed_hex[..24]), "{line}");
        assert!(!format!("{key:?}{sealed:?}{opened:?}{error}").contains(token));
    }
}
