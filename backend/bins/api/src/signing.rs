//! The two synchronous ports `api` implements itself: the HMAC behind signed image URLs
//! ([storage 3]) and the in-memory species photo index ([contracts 9], species).

use std::collections::HashMap;

use hmac::{Hmac, KeyInit, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

use domain::{
    garden::species::SpeciesPhoto,
    id::SpeciesId,
    ports::{SpeciesPhotos, UrlSigner},
    url::SigningInput,
};

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 over `SigningInput::canonical()` (`"GET\n{path}\n{exp}"`), lowercase hex
/// (`domain::url`; domain open question 11). The key is `API_URL_SIGNING_KEY`.
pub struct HmacSigner {
    key: SecretString,
    public_base: String,
}

impl HmacSigner {
    /// A signer for URLs under `public_base`.
    pub fn new(key: SecretString, public_base: String) -> Self {
        Self { key, public_base }
    }

    fn mac(&self, input: &SigningInput) -> HmacSha256 {
        // reason: HMAC accepts keys of any length, so `new_from_slice` can't fail.
        let mut mac = HmacSha256::new_from_slice(self.key.expose_secret().as_bytes())
            .expect("HMAC takes any key length");
        mac.update(input.canonical().as_bytes());
        mac
    }
}

impl UrlSigner for HmacSigner {
    fn signature(&self, input: &SigningInput) -> String {
        hex(&self.mac(input).finalize().into_bytes())
    }

    fn verify(&self, input: &SigningInput, sig: &str) -> bool {
        // `verify_slice` compares in constant time and checks the length.
        unhex(sig).is_some_and(|bytes| self.mac(input).verify_slice(&bytes).is_ok())
    }

    fn public_base(&self) -> &str {
        &self.public_base
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            let digit = |c: u8| char::from(c).to_digit(16);
            u8::try_from(digit(pair[0])? * 16 + digit(pair[1])?).ok()
        })
        .collect()
}

/// The species reference photos, held in memory and refreshed only by a restart (the knowledge
/// base changes only with a migration). Loaded from `KnowledgeRepo::load_knowledge` at start.
pub struct PhotoIndex(HashMap<SpeciesId, Vec<SpeciesPhoto>>);

impl PhotoIndex {
    /// From the knowledge snapshot's `species_photos`.
    pub fn new(photos: Vec<(SpeciesId, Vec<SpeciesPhoto>)>) -> Self {
        Self(photos.into_iter().collect())
    }
}

impl SpeciesPhotos for PhotoIndex {
    fn photos(&self, species: &SpeciesId) -> Vec<SpeciesPhoto> {
        self.0.get(species).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        id::ScanId,
        url::{ImagePath, Signer, verify_signed},
        wire::Timestamp,
    };

    #[test]
    fn signed_urls_verify_until_they_expire() {
        let signer = HmacSigner::new(
            SecretString::from("k".repeat(32)),
            "https://api.example.org".to_owned(),
        );
        let photos = PhotoIndex::new(vec![]);
        let now: Timestamp = "2026-10-18T14:00:00Z".parse().unwrap();
        let scan: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
        let path = ImagePath::ScanImage {
            scan,
            image: "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap(),
        };
        let url = Signer::new(&signer, &photos, now).url(&path);
        let query = url.as_str().split_once('?').unwrap().1;
        let (exp, sig) = query.split_once('&').unwrap();
        let exp: i64 = exp.trim_start_matches("exp=").parse().unwrap();
        let sig = sig.trim_start_matches("sig=");
        assert_eq!(sig.len(), 64);
        assert!(verify_signed(&signer, &path, exp, sig, now).is_ok());
        let later = now.plus(std::time::Duration::from_mins(16));
        assert!(verify_signed(&signer, &path, exp, sig, later).is_err());
        let mut tampered = sig.to_owned();
        tampered.replace_range(0..1, if sig.starts_with('0') { "1" } else { "0" });
        assert!(verify_signed(&signer, &path, exp, &tampered, now).is_err());
    }
}
