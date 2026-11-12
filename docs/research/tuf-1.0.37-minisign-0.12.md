# TUF 1.0.37 and minisign 0.12 reference

This page records how The Update Framework (TUF) specification 1.0.37 splits signing trust into roles, rotates keys, and defends against named attacks. It also records the minisign 0.12 signature format, the Rust crates `minisign` 0.10.0 and `minisign-verify` 0.3.0, and how Nextcloud and Sigstore revoke signing trust, with and without a network. deployarr add-ons arrive as files an admin selects in the web UI, and the server may have no internet access. Use this page when you design how the server decides that an add-on package is trusted, and how a compromised publisher key stops being trusted.

## Sources

Each source has a short label. Citations name the label, then a path and a line range in the pinned tag.

| Label | Source | Version | How it was obtained |
| --- | --- | --- | --- |
| tuf | `https://github.com/theupdateframework/specification`, file `tuf-spec.md` | tag `v1.0.37`, commit `d4cb70582ba507b9265672792c0fc8a5f390b851`, dated 2026-10-08 in the spec header (`tuf-spec.md:6`, `tuf-spec.md:19`) | `git clone --depth 1 --branch v1.0.37`. The rendered spec is `https://theupdateframework.github.io/specification/v1.0.37/`. `v1.0.37` was the newest tag on 2026-11-12. |
| minisign | `https://github.com/jedisct1/minisign` | tag `0.12`, commit `b85e15d45ac9eab34e44596fd309f5b07db9545c`, tagged 2025-01-15 | `git clone --depth 1 --branch 0.12`. `0.12` was the newest tag on 2026-11-12. |
| minisign-docs | `https://jedisct1.github.io/minisign/`, sections "Trusted comments" and "Signature format" | fetched 2026-11-12, describes 0.12 | `curl` |
| rust-minisign | `https://github.com/jedisct1/rust-minisign`, crate `minisign` | tag `0.10.0`, commit `4e0212f55f4e7075df384864b665b0129b78cdf3`, tagged 2026-09-25 | `git clone`. `cargo search minisign` listed 0.10.0 as the newest. |
| minisign-verify | `https://github.com/jedisct1/rust-minisign-verify`, crate `minisign-verify` | tag `0.3.0`, commit `96b921399b87b29e29cb5dc948b99b465ca58489`, tagged 2026-09-25 | `git clone`. `cargo search` listed 0.3.0 as the newest. |
| nextcloud | `https://github.com/nextcloud/server` | tag `v35.0.3`, commit `7764ef4927e5f85071e495ce7602ccd247bb6b30` | sparse clone of `resources/codesigning`, `lib/private/IntegrityCheck`, and `lib/private/Installer.php`. `v35.0.3` was the newest stable tag on 2026-11-12. |
| sigstore-rs | `https://github.com/sigstore/sigstore-rs`, crate `sigstore` | tag `v0.15.0`, commit `3c6899b12be667a3f3c817579242d5fde78bb7f1`, tagged 2026-11-05 | `git clone` |
| tough | `https://github.com/awslabs/tough`, crate `tough` | tag `tough-v0.24.0`, commit `98d8eb8b2ce63515d9b4981c938ef6453c5b5771`, tagged 2026-07-10 | `git clone`. `sigstore` 0.15.0 depends on `tough` 0.24 (sigstore-rs `Cargo.toml:169`). |

Earlier add-on pages cover related ground. [Grafana](grafana-plugins-13.3.1.md#key-management) revokes a key by dropping it from a key list it downloads every 10 days. [Mattermost](mattermost-plugins-12.1.0.md#signing-and-verification) verifies OpenPGP signatures against compiled-in and admin-added keys. [go-plugin](go-plugin-1.8.0.md#trust) records how Terraform checks a signed `SHA256SUMS` file. This page does not repeat them.

## TUF roles

TUF has four required top-level roles and one optional role. Every role has one or more keys and a threshold of signatures (tuf `tuf-spec.md:283-293`, `tuf-spec.md:739-740`).

| Role | Signs | Key placement | What it stops |
| --- | --- | --- | --- |
| root | the keys and threshold of every top-level role, its own included (tuf `tuf-spec.md:297-298`, `tuf-spec.md:675-679`) | offline. The client ships with trusted root keys (tuf `tuf-spec.md:300-304`). | key compromise of any top-level role, through rotation (tuf `tuf-spec.md:303-307`, `tuf-spec.md:1590-1592`) |
| targets | file metadata, not the files. Each entry has a length, hashes, and an optional `custom` object (tuf `tuf-spec.md:318-320`, `tuf-spec.md:959-1009`) | offline (tuf `tuf-spec.md:1584-1586`) | arbitrary and wrong software (tuf `tuf-spec.md:1505-1514`, `tuf-spec.md:1557-1564`) |
| snapshot | the current version of every targets metadata file (tuf `tuf-spec.md:338-344`) | offline (tuf `tuf-spec.md:1584-1586`). Sigstore gives it threshold 1 and the same single key as timestamp (sigstore-rs `trust_root/prod/root.json:113-120`, `:131-138`), so that key is online (inferred). | mix-and-match (tuf `tuf-spec.md:343-344`) |
| timestamp | a fresh statement with the hash of the snapshot file (tuf `tuf-spec.md:346-352`) | online, signed by an automated process (tuf `tuf-spec.md:348-352`) | replay of out-of-date metadata, which is indefinite freeze (tuf `tuf-spec.md:348-350`) |
| mirrors | an optional signed mirror list (tuf `tuf-spec.md:356-359`) | online allowed (tuf `tuf-spec.md:1584-1586`) | contact with the wrong mirrors, which the spec calls a minimal risk (tuf `tuf-spec.md:361-364`) |

- The targets role can delegate full trust or trust for a subset of paths to another role with its own keys and threshold. Delegations can nest (tuf `tuf-spec.md:322-333`).
- A delegation names its keys, a threshold, and `paths` or `path_hash_prefixes`. A `terminating` delegation stops the search, so no later role can make claims about a matching target (tuf `tuf-spec.md:1011-1055`).
- Revoking a delegation means signing new delegating metadata without it (tuf `tuf-spec.md:335-336`). Replacing a delegated developer key works the same way (tuf `tuf-spec.md:1618-1620`).
- The `custom` object is opaque to TUF. It may hold version numbers, dependencies, or requirements for the application (tuf `tuf-spec.md:1000-1009`).
- The Sigstore production root embedded in `sigstore` 0.15.0, version 12, uses 3 of 5 keys for root and targets and 1 of 1 for snapshot and timestamp (sigstore-rs `trust_root/prod/root.json:103-136`, `:141`).

## Thresholds and key rotation

- A signature counts toward a threshold only once per key ID, even when the `signatures` list repeats a key ID (tuf `tuf-spec.md:1335-1341`). A key ID is the SHA-256 of the key's canonical form (tuf `tuf-spec.md:538-541`).
- To rotate root keys, the root role signs a new `root.json` with both the old and the new keys. The repository serves each version by number, such as `3.root.json` (tuf `tuf-spec.md:1590-1595`).
- A client walks the root versions one at a time. Version N+1 must carry a threshold of signatures from the keys in version N and a threshold from its own keys (tuf `tuf-spec.md:1331-1345`, `tuf-spec.md:1597-1611`).
- Version N+1 must be exactly N plus 1, or the client reports a rollback attack (tuf `tuf-spec.md:1346-1349`). The client writes each accepted root to storage (tuf `tuf-spec.md:1355-1359`).
- An old root's expiry does not stop the walk. The client checks only the final root's expiry (tuf `tuf-spec.md:1304-1308`, `tuf-spec.md:1352-1353`, `tuf-spec.md:1363-1366`).
- A client caps how many root versions it fetches and how many bytes each may be (tuf `tuf-spec.md:1321-1329`).
- If a threshold of root keys is compromised, the spec says the root keys should be updated out of band, and calls safe recovery nearly impossible (tuf `tuf-spec.md:307-313`).
- The spec recommends that implementations support the `rsa`, `ed25519`, and `ecdsa` key types, and lets adopters define others (tuf `tuf-spec.md:583-597`).

## Attacks and how each holds offline

The spec lists the attacks TUF defends against (tuf `tuf-spec.md:147-200`). The table covers eight of the ten. The last column is this page's reading of whether the defense survives on a server that never reaches a repository (inferred).

| Attack | TUF defense | Offline |
| --- | --- | --- |
| Arbitrary installation (tuf `tuf-spec.md:156-158`) | each metadata file needs a threshold of keys from the trusted root (tuf `tuf-spec.md:1448-1458`, `tuf-spec.md:1505-1514`) | holds. It needs only the shipped root and signatures. |
| Wrong software (tuf `tuf-spec.md:199-200`) | the target must match the hashes in signed targets metadata, and its download stops at the signed length (tuf `tuf-spec.md:1557-1564`) | holds |
| Endless data (tuf `tuf-spec.md:160-162`) | every download is capped by a signed length or a fixed limit (tuf `tuf-spec.md:1321-1324`, `tuf-spec.md:1562-1565`) | holds when the server caps upload size |
| Rollback (tuf `tuf-spec.md:189-191`) | versions never go down, and a known targets file stays listed (tuf `tuf-spec.md:725-728`, `tuf-spec.md:1465-1471`) | holds if the server stores the highest version it has seen |
| Mix-and-match (tuf `tuf-spec.md:185-187`) | each targets file must match the version and any hashes that the snapshot lists for it (tuf `tuf-spec.md:1496-1503`, `tuf-spec.md:1516-1519`) | holds if one signed file carries all trust data, so there is nothing to mix |
| Indefinite freeze (tuf `tuf-spec.md:178-180`) | every file expires, and the online timestamp role re-signs often (tuf `tuf-spec.md:730-734`, `tuf-spec.md:1417-1421`) | fails. Without a fresh timestamp the server cannot tell stale data from current data. |
| Fast-forward (tuf `tuf-spec.md:168-177`) | after a timestamp or snapshot key rotation, the client deletes its stored timestamp and snapshot (tuf `tuf-spec.md:1368-1377`) | not relevant without those roles |
| Key compromise (tuf `tuf-spec.md:193-197`) | separate roles, thresholds, and rotation by root (tuf `tuf-spec.md:292-293`, `tuf-spec.md:303-307`, `tuf-spec.md:1590-1592`) | holds for rotation that arrives with a new file |

- The spec admits that an attacker who controls the repository can freeze clients by withholding new root metadata, until the latest root the client has expires (tuf `tuf-spec.md:1613-1616`).
- The spec's non-goals exclude files a person downloads by hand. "People still need to use other means to verify" them (tuf `tuf-spec.md:84`, `tuf-spec.md:98-101`). An admin who selects a package file is that case (inferred).
- TUF lets an implementation pick any metadata format. Signatures cover a canonical form, and the examples use canonical JSON (tuf `tuf-spec.md:507-516`, `tuf-spec.md:547`). Unknown fields must be kept and included when hashing or verifying signatures (tuf `tuf-spec.md:498-503`).
- The spec lists bogus clocks as an open question (tuf `tuf-spec.md:1735-1739`).

### How Rust TUF clients handle offline use

- `tough` offers `ExpirationEnforcement::Unsafe`, documented as "available for certain offline use cases" without TUF's security guarantees (tough `tough/src/lib.rs:83-93`). Its doc example loads a repository from local file URLs with `FilesystemTransport` and `Unsafe` (tough `tough/src/lib.rs:148-172`).
- `tough` stores the latest system time it has seen in `latest_known_time.json` and fails when the clock moves backward (tough `tough/src/datastore.rs:88-118`).
- `sigstore` 0.15.0 builds its trust root through `tough` with `Safe` expiry and the Sigstore TUF CDN, so `SigstoreTrustRoot::new` needs the network (sigstore-rs `src/trust/sigstore/mod.rs:68-89`). The embedded `root.json` is the starting TUF root. `fetch_target` reads `trusted_root.json` from the disk cache first, then from the embedded copy, then from the TUF repository (sigstore-rs `src/trust/sigstore/constants.rs:16-36`, `src/trust/sigstore/mod.rs:77-80`, `src/trust/sigstore/mod.rs:152-165`).
- For offline use, `from_trusted_root_json_unchecked` takes a trust root with no checks. The caller "must ensure that the data is trustworthy" (sigstore-rs `src/trust/sigstore/mod.rs:91-102`).
- The bundle verifier takes an `offline` flag. It checks the certificate chain, the SCT, the identity policy, the signature, and that the Rekor entry matches. It does not check the Merkle inclusion proof or the signed entry timestamp in either mode. Both steps are marked `TODO` (sigstore-rs `src/bundle/verify/verifier.rs:158-262`). The flag only decides whether a bundle with no inclusion proof is refused. Online, such a bundle fails, because online Rekor fetching is not implemented (sigstore-rs `src/bundle/verify/models.rs:555-560`).

## minisign signature format

A `.minisig` file has four lines (minisign-docs "Signature format", minisign `src/minisign.c:154-235`):

| Line | Content | Signed |
| --- | --- | --- |
| 1 | `untrusted comment: <text>` | no |
| 2 | base64 of the algorithm (2 bytes), the key ID (8 bytes), and the signature (64 bytes) | is the signature |
| 3 | `trusted comment: <text>` | yes, by line 4 |
| 4 | base64 of the global signature, Ed25519 over the line 2 signature followed by the trusted comment text | is the signature |

- Algorithm `ED` is prehashed. The signature is Ed25519 over BLAKE2b-512 of the file. Algorithm `Ed` is the legacy mode, Ed25519 over the whole file (minisign `src/minisign.h:9-10`, `src/minisign.c:75-100`, `src/minisign.c:563-566`). The docs say new implementations "must use the hashed signature format" and should not accept legacy by default (minisign-docs "Signature format").
- The `minisign` 0.12 CLI accepts legacy signatures unless `-H` is given (minisign `src/minisign.c:852`, `src/minisign.c:903-904`, `src/minisign.c:458-463`).
- Verification checks the key ID, then the file signature, then the global signature over the trusted comment (minisign `src/minisign.c:465-495`). A changed trusted comment fails with "Comment signature verification failed" (minisign `src/minisign.c:488-494`).
- The docs suggest the trusted comment for file names, timestamps, resource IDs, or version numbers "to prevent downgrade attacks" (minisign-docs "Trusted comments"). The CLI default is `timestamp:<unix time>`, a tab, `file:<name>`, and a tab and `hashed` for prehashed signatures (minisign `src/minisign.c:527-539`). The trusted comment is capped at 8192 bytes with its prefix (minisign `src/minisign.h:8`, `src/minisign.c:585-587`).
- The timestamp in a trusted comment is whatever the signer's clock said. Nothing outside the signer vouches for it (inferred from `src/minisign.c:527-539`).
- The key ID is 8 random bytes chosen when the key is made. It is not derived from the public key (minisign `src/minisign.c:685`). Anyone who makes a key can choose any key ID (inferred, because the ID is only stored next to the key).
- One `.minisig` file holds one signature by one key (minisign `src/minisign.h:51-55`). The format has no threshold. Several signers mean several files (inferred).
- A public key is the algorithm `Ed`, the key ID, and the 32-byte Ed25519 key (minisign `src/minisign.h:31-34`, `src/minisign.h:46-49`, `src/minisign.c:693`).

## Rust crates

### `minisign-verify` 0.3.0

- It has no dependencies. Its `Cargo.toml` has no `[dependencies]` section, and it carries its own Ed25519, SHA-512, and BLAKE2b code (minisign-verify `Cargo.toml:1-17`, `src/crypto/ed25519.rs`, `src/crypto/sha512.rs`, `src/crypto/blake2b.rs`).
- `PublicKey::verify(bin, signature, allow_legacy)` checks the key ID, prehashes when the signature says `ED`, and rejects legacy signatures with `UnexpectedAlgorithm` when `allow_legacy` is false (minisign-verify `src/lib.rs:351-371`).
- `verify_stream` returns a `StreamVerifier` with `update` and `finalize`, so a large file never has to be in memory. It refuses legacy signatures with `UnsupportedLegacyMode` (minisign-verify `src/lib.rs:374-412`).
- Both paths verify the global signature over the trusted comment, so `Ok(())` means the trusted comment is authentic too (minisign-verify `src/lib.rs:334-346`). `Signature::trusted_comment` returns its text without the `trusted comment: ` prefix (minisign-verify `src/lib.rs:276-279`).
- Its Ed25519 check rejects a signature whose `S` is not below the group order and an identity public key (minisign-verify `src/crypto/ed25519.rs:10-30`).
- `PublicKey::from_base64` accepts `ED` as well as `Ed` as a public key algorithm (minisign-verify `src/lib.rs:289-300`).
- Errors are a plain enum, including `UnexpectedKeyId`, `InvalidSignature`, and `UnexpectedAlgorithm` (minisign-verify `src/lib.rs:116-132`).

### `minisign` 0.10.0

- `sign` always writes prehashed signatures. The default trusted comment is only `timestamp:<unix time>` (rust-minisign `src/lib.rs:85-95`, `src/lib.rs:125`).
- It hashes 64 random bytes into each Ed25519 nonce, so two signatures of the same file differ (rust-minisign `src/lib.rs:98-108`, `src/crypto/ed25519.rs:86-98`). The minisign README says non-deterministic signatures stay interoperable (minisign `README.md:133-139`).
- `verify` takes `allow_legacy` and checks the global signature when the signature box has one (rust-minisign `src/lib.rs:140-200`). A `SignatureBox` converted from `SignatureBones` has no trusted comment, and `verify` then skips that check (rust-minisign `src/signature_bones.rs:48-59`, `src/lib.rs:193`).
- It depends on `getrandom`, `scrypt`, and `ct-codecs`, and on Windows and Unix also on `rpassword` (rust-minisign `Cargo.toml:17-28`). It is a signing tool dependency, not a verifier dependency.

## Offline revocation in shipping products

### Nextcloud 35.0.3

- The server ships a root certificate and a certificate revocation list in `resources/codesigning`, as `root.crt` and `root.crl` (nextcloud `resources/codesigning/`).
- `openssl crl -text` on the shipped `root.crl` shows the issuer "Nextcloud Code Signing Intermediate Authority", CRL number 4232, 141 revoked serial numbers, a last update of 2026-11-12, and a next update of 2036-09-20. The ten-year window means an offline server never sees the list expire (inferred from those dates).
- An app download checks the CRL signature against the shipped root, then refuses a certificate whose serial is on the list (nextcloud `lib/private/Installer.php:182-200`). The CRL check runs before the chain check (nextcloud `lib/private/Installer.php:202-210`).
- At install, the certificate must chain to the root, and its CN must equal the app ID. A certificate for one app cannot install another (nextcloud `lib/private/Installer.php:202-230`, `lib/private/IntegrityCheck/Checker.php:28-35`).
- The installer checks a SHA-512 signature over the downloaded tarball with the certificate's public key before extraction (nextcloud `lib/private/Installer.php:242-254`).
- The integrity checker that re-checks installed apps tests the chain and the CN but not the CRL. It also accepts a certificate with CN `core` for any app (nextcloud `lib/private/IntegrityCheck/Checker.php:295-311`). A search of `Checker.php` for `crl` and `revok` finds nothing, so a revoked certificate's installed app keeps passing (inferred).
- A new revocation reaches a server only through a server update, because the list is a file in the release (inferred from its location).

### Sigstore through `sigstore` 0.15.0

- Sigstore revokes and rotates through TUF on an online CDN (sigstore-rs `src/trust/sigstore/constants.rs:16-17`). Offline, the crate needs an unchecked trust root. Its verifier does not check the transparency log proofs, online or offline (see [How Rust TUF clients handle offline use](#how-rust-tuf-clients-handle-offline-use)). It also needs Fulcio certificates and an identity policy (sigstore-rs `src/bundle/verify/verifier.rs:112-124`, `:210`). Nothing in it targets a server with no internet access (inferred).

## What changes in deployarr

1. **Ship a signed trust bundle as the root of add-on trust.** Copy the TUF root and targets split from [TUF roles](#tuf-roles). The server binary embeds the deployarr root public keys. The root keys sign one file, `addon-trust.json`, that lists publisher public keys with their scopes, revoked keys, and revoked package hashes. Keep the root keys offline. Drop the snapshot and timestamp roles, because no online repository exists. One file with one version number leaves nothing to mix and match.
2. **Scope each publisher key to add-on IDs.** A trust bundle entry names the ID patterns a key may sign, such as `deployarr.*` for the project key and `acme.*` for a vendor. Copy TUF's `paths` delegations and Nextcloud's CN-equals-app-ID rule ([Nextcloud 35.0.3](#nextcloud-3503)). Make every delegation terminating, so a second key never vouches for the same ID.
3. **Use minisign prehashed signatures, verified with `minisign-verify` 0.3.0.** It has no dependencies and streams large packages, such as driver catalogs ([`minisign-verify` 0.3.0](#minisign-verify-030)). Always pass `allow_legacy = false`, unlike the minisign CLI default. Sign the raw bytes of the package and of `addon-trust.json`, so no canonical JSON is needed. Use the `minisign` crate only in the publisher signing tool under `xtask/`.
4. **Put the add-on identity in the trusted comment and check it against the manifest.** Define a fixed format, such as `deployarr-addon id=<id> version=<semver> sha256=<hex>`. Reject a package whose manifest ID or version differs from the signed comment. This blocks a signed package being installed under another ID or version, which is the wrong-software attack in [Attacks](#attacks-and-how-each-holds-offline).
5. **Never select a key by its 8-byte minisign key ID alone.** The ID is random and chosen by the key's owner ([minisign signature format](#minisign-signature-format)). Use it to find candidate keys, then match the full public key in the trust store. Revoke by full public key.
6. **Make root rotation and thresholds follow TUF.** Sign `addon-trust.json` with k of n root keys as k separate `.minisig` files. Count each distinct public key once. A new root key list needs k signatures from the old list and k from the new one, with a version exactly one higher ([Thresholds and key rotation](#thresholds-and-key-rotation)).
7. **Block rollback with stored versions.** The server stores the highest `addon-trust.json` version it has accepted and refuses a lower one. It stores the highest installed version of each add-on and asks the admin for an explicit, logged confirmation before a downgrade.
8. **Deliver revocations three ways and keep the highest version.** Ship `addon-trust.json` in every server release, as Nextcloud ships `root.crl`. Let an admin import a newer one from a file in the web UI, for networks with no internet access. Let an online server fetch it on a schedule as an option. Grafana's fetched key list is the online model ([Grafana](grafana-plugins-13.3.1.md#key-management)).
9. **Accept the freeze risk openly instead of enforcing expiry.** An expiry date would break servers that stay offline. Nextcloud's CRL runs ten years, and `tough` offers `Unsafe` mode for offline use. Show the bundle's signing date and age in the web UI and warn when it is old. Do not trust the date for security, because it comes from the signer's clock.
10. **Check revocation at install and at every server start.** When a key or package hash in the bundle matches an installed add-on, disable the add-on and show why. Avoid Nextcloud's gap, where the integrity checker never reads the CRL.
11. **Revoke whole keys, not keys after a date.** minisign has no trusted signing time, so the server cannot tell a package signed before a compromise from one signed after it. To keep known good packages after a key leak, list their SHA-256 hashes in the bundle as allowed, or re-sign them with a new key.
12. **Cap sizes before hashing.** Limit the package size, the `.minisig` size, and the trusted comment length before any work. This is TUF's endless-data rule.
13. **Treat admin-added keys as local trust under the same rules.** An admin key gets an ID scope, shows as local in the UI, and is still subject to the bundle's revoked key list.
14. **Do not build on Sigstore for offline servers.** `sigstore` 0.15.0 needs an online trust root or an unchecked one, and its verifier does not check the log proofs.

## Open questions

- Who holds the deployarr root keys, and what are k and n? TUF leaves key storage to the implementer (tuf `tuf-spec.md:1586-1588`).
- Will third parties publish add-ons with their own keys, or does the project sign every add-on? The answer decides whether the bundle needs publisher scopes at the start.
- How old may `addon-trust.json` get before the UI warns, and should an admin be able to silence the warning?
- Can trust bundle parsing and verification live in `deployarr-core`? `minisign-verify` has no dependencies, so it should build for `wasm32` (inferred, not tested).
