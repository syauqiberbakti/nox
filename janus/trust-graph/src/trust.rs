/*
 *   MIT License
 *
 *   Copyright (c) 2020 Fluence Labs Limited
 *
 *   Permission is hereby granted, free of charge, to any person obtaining a copy
 *   of this software and associated documentation files (the "Software"), to deal
 *   in the Software without restriction, including without limitation the rights
 *   to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 *   copies of the Software, and to permit persons to whom the Software is
 *   furnished to do so, subject to the following conditions:
 *
 *   The above copyright notice and this permission notice shall be included in all
 *   copies or substantial portions of the Software.
 *
 *   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 *   IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 *   FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 *   AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 *   LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 *   OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 *   SOFTWARE.
 */

use crate::ed25519::PublicKey;
use crate::key_pair::{KeyPair, Signature};
use derivative::Derivative;
use std::convert::TryInto;
use std::time::Duration;

pub const SIGNATURE_LEN: usize = 64;
pub const PUBLIC_KEY_LEN: usize = 32;
pub const EXPIRATION_LEN: usize = 8;
pub const ISSUED_LEN: usize = 8;
pub const TRUST_LEN: usize = SIGNATURE_LEN + PUBLIC_KEY_LEN + EXPIRATION_LEN + ISSUED_LEN;

/// One element in chain of trust in a certificate.
/// TODO delete pk from Trust (it is already in a trust node)
#[derive(Clone, PartialEq, Derivative)]
#[derivative(Debug)]
pub struct Trust {
    /// For whom this certificate is issued
    #[derivative(Debug(format_with = "show_pubkey"))]
    pub issued_for: PublicKey,
    /// Expiration date of a trust.
    pub expires_at: Duration,
    /// Signature of a previous trust in a chain.
    /// Signature is self-signed if it is a root trust.
    #[derivative(Debug(format_with = "show_sig"))]
    pub signature: Signature,
    /// Creation time of a trust
    pub issued_at: Duration,
}

fn show_pubkey(key: &PublicKey, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    write!(f, "{}", bs58::encode(key.encode()).into_string())
}

fn show_sig(sig: &Signature, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    write!(f, "{}", bs58::encode(sig).into_string())
}

impl Trust {
    #[allow(dead_code)]
    pub fn new(
        issued_for: PublicKey,
        expires_at: Duration,
        issued_at: Duration,
        signature: Signature,
    ) -> Self {
        Self {
            issued_for,
            expires_at,
            issued_at,
            signature,
        }
    }

    pub fn create(
        issued_by: &KeyPair,
        issued_for: PublicKey,
        expires_at: Duration,
        issued_at: Duration,
    ) -> Self {
        let msg = Self::signature_bytes(&issued_for, expires_at, issued_at);

        let signature = issued_by.sign(&msg);

        Self {
            issued_for,
            expires_at,
            signature,
            issued_at,
        }
    }

    /// Verifies that authorization is cryptographically correct.
    pub fn verify(trust: &Trust, issued_by: &PublicKey, cur_time: Duration) -> Result<(), String> {
        if trust.expires_at < cur_time {
            return Err("Trust in chain is expired.".to_string());
        }

        let msg = Self::signature_bytes(&trust.issued_for, trust.expires_at, trust.issued_at);

        KeyPair::verify(issued_by, &msg, trust.signature.as_slice())?;

        Ok(())
    }

    fn signature_bytes(pk: &PublicKey, expires_at: Duration, issued_at: Duration) -> [u8; 48] {
        let pk_encoded = pk.encode();
        let expires_at_encoded: [u8; 8] = (expires_at.as_millis() as u64).to_le_bytes();
        let issued_at_encoded: [u8; 8] = (issued_at.as_millis() as u64).to_le_bytes();
        let mut msg = [0; 48];

        msg[..32].clone_from_slice(&pk_encoded[..32]);
        msg[33..40].clone_from_slice(&expires_at_encoded[0..7]);
        msg[41..48].clone_from_slice(&issued_at_encoded[0..7]);

        msg
    }

    /// Encode the trust into a byte array
    #[allow(dead_code)]
    pub fn encode(&self) -> Vec<u8> {
        let mut vec = Vec::with_capacity(TRUST_LEN);
        vec.extend_from_slice(&self.issued_for.encode());
        vec.extend_from_slice(&self.signature.as_slice());
        vec.extend_from_slice(&(self.expires_at.as_millis() as u64).to_le_bytes());
        vec.extend_from_slice(&(self.issued_at.as_millis() as u64).to_le_bytes());

        vec
    }

    /// Decode a trust from a byte array as produced by `encode`.
    #[allow(dead_code)]
    pub fn decode(arr: &[u8]) -> Result<Self, String> {
        if arr.len() != TRUST_LEN {
            return Err(
                format!("Trust length should be 104: public key(32) + signature(64) + expiration date(8), was: {}", arr.len()),
            );
        }

        let pk = PublicKey::decode(&arr[0..PUBLIC_KEY_LEN]).map_err(|err| err.to_string())?;
        let signature = &arr[PUBLIC_KEY_LEN..PUBLIC_KEY_LEN + SIGNATURE_LEN];

        let expiration_bytes =
            &arr[PUBLIC_KEY_LEN + SIGNATURE_LEN..PUBLIC_KEY_LEN + SIGNATURE_LEN + EXPIRATION_LEN];
        let expiration_date = u64::from_le_bytes(expiration_bytes.try_into().unwrap());
        let expiration_date = Duration::from_millis(expiration_date);

        let issued_bytes = &arr[PUBLIC_KEY_LEN + SIGNATURE_LEN + EXPIRATION_LEN..TRUST_LEN];
        let issued_date = u64::from_le_bytes(issued_bytes.try_into().unwrap());
        let issued_date = Duration::from_millis(issued_date);

        Ok(Self {
            issued_for: pk,
            signature: signature.to_vec(),
            expires_at: expiration_date,
            issued_at: issued_date,
        })
    }

    fn bs58_str_to_vec(str: &str, field: &str) -> Result<Vec<u8>, String> {
        bs58::decode(str).into_vec().map_err(|e| {
            format!(
                "Cannot decode `{}` from base58 format in the trust '{}': {}",
                field, str, e
            )
        })
    }

    fn str_to_duration(str: &str, field: &str) -> Result<Duration, String> {
        let millis = str.parse().map_err(|e| {
            format!(
                "Cannot parse `{}` field in the trust '{}': {}",
                field, str, e
            )
        })?;
        Ok(Duration::from_millis(millis))
    }

    pub fn convert_from_strings(
        issued_for: &str,
        signature: &str,
        expires_at: &str,
        issued_at: &str,
    ) -> Result<Self, String> {
        // PublicKey
        let issued_for_bytes = Self::bs58_str_to_vec(issued_for, "issued_for")?;
        let issued_for = PublicKey::decode(issued_for_bytes.as_slice()).map_err(|e| {
            format!(
                "Cannot decode the public key: {} in the trust '{}'",
                issued_for, e
            )
        })?;

        // 64 bytes signature
        let signature = Self::bs58_str_to_vec(signature, "signature")?;

        // Duration
        let expires_at = Self::str_to_duration(expires_at, "expires_at")?;

        // Duration
        let issued_at = Self::str_to_duration(issued_at, "issued_at")?;

        Ok(Trust::new(issued_for, expires_at, issued_at, signature))
    }
}

impl ToString for Trust {
    fn to_string(&self) -> String {
        let issued_for = bs58::encode(self.issued_for.encode()).into_string();
        let signature = bs58::encode(self.signature.as_slice()).into_string();
        let expires_at = (self.expires_at.as_millis() as u64).to_string();
        let issued_at = (self.issued_at.as_millis() as u64).to_string();

        format!(
            "{}\n{}\n{}\n{}",
            issued_for, signature, expires_at, issued_at
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_revoke_and_validate() {
        let truster = KeyPair::generate();
        let trusted = KeyPair::generate();

        let current = Duration::new(100, 0);
        let duration = Duration::new(1000, 0);
        let issued_at = Duration::new(10, 0);

        let trust = Trust::create(&truster, trusted.public_key(), duration, issued_at);

        assert_eq!(
            Trust::verify(&trust, &truster.public_key(), current).is_ok(),
            true
        );
    }

    #[test]
    fn test_validate_corrupted_revoke() {
        let truster = KeyPair::generate();
        let trusted = KeyPair::generate();

        let current = Duration::new(1000, 0);
        let issued_at = Duration::new(10, 0);

        let trust = Trust::create(&truster, trusted.public_key(), current, issued_at);

        let corrupted_duration = Duration::new(1234, 0);
        let corrupted_trust = Trust::new(
            trust.issued_for,
            trust.expires_at,
            corrupted_duration,
            trust.signature,
        );

        assert!(Trust::verify(&corrupted_trust, &truster.public_key(), current).is_err());
    }

    #[test]
    fn test_encode_decode() {
        let truster = KeyPair::generate();
        let trusted = KeyPair::generate();

        let current = Duration::new(1000, 0);
        let issued_at = Duration::new(10, 0);

        let trust = Trust::create(&truster, trusted.public_key(), current, issued_at);

        let encoded = trust.encode();
        let decoded = Trust::decode(encoded.as_slice()).unwrap();

        assert_eq!(trust, decoded);
    }
}
