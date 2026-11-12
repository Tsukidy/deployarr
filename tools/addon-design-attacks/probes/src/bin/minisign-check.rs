//! Supported claim: `minisign-verify` 0.3.0 verifies a prehashed signature with
//! `allow_legacy = false`. Rewritten. fable/mini did not survive.
//!
//! The `minisign` crate 0.10.0 makes a key pair and signs a payload. Its `sign` always writes a
//! prehashed signature. `minisign-verify` 0.3.0 then checks the text of that signature.

use minisign_verify::{PublicKey, Signature};

fn main() {
    let payload = b"payload.zip bytes".to_vec();
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let trusted = "id=acme.join\tversion=1.1.0";
    let sig = minisign::sign(Some(&kp.pk), &kp.sk, &payload[..], Some(trusted), None).unwrap().to_string();
    let algorithm = sig.lines().nth(1).map(|l| &l[..3]).unwrap_or("");

    let pk = PublicKey::from_base64(&kp.pk.to_base64()).unwrap();
    let other_pk = PublicKey::from_base64(&other.pk.to_base64()).unwrap();
    let decoded = Signature::decode(&sig).unwrap();

    println!("signature from minisign 0.10.0: base64 starts with {algorithm:?}, trusted comment {:?}", decoded.trusted_comment());
    println!("verify, allow_legacy = false: {:?}", pk.verify(&payload, &decoded, false));
    let mut stream = pk.verify_stream(&decoded).unwrap();
    stream.update(&payload);
    println!("verify_stream: {:?}", stream.finalize());
    let mut changed = payload.clone();
    changed[0] ^= 1;
    println!("one payload bit changed: {:?}", pk.verify(&changed, &decoded, false));
    let edited = sig.replace("version=1.1.0", "version=9.9.9");
    println!("trusted comment edited: {:?}", pk.verify(&payload, &Signature::decode(&edited).unwrap(), false));
    println!("another publisher's key: {:?}", other_pk.verify(&payload, &decoded, false));
}
