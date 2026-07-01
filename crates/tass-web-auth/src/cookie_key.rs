//! The private-cookie signing [`Key`] (axum-extra): load it from
//! `state_dir()/cookie/` (64 bytes), generating on first run.
//!
//! The session cookie holds only an encoded `SessionKey` (never OAuth tokens),
//! signed/encrypted with this key. Tokens + the per-session DPoP key live in
//! the turso session store — that durable separation is what makes browser
//! sessions safe.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use axum_extra::extract::cookie::Key;
use tass_config::ServiceConfig;

/// The cookie-key length jacquard-axum's `PrivateCookieJar` expects (HMAC-SHA256
/// master key). Matches `axum_extra::extract::cookie::Key`'s 64-byte key.
pub(crate) const KEY_LEN: usize = 64;

/// Load the active cookie key, generating it on first run.
///
/// `cookie_paths[0]` is the active signer; the tail models a keyring, honoured
/// once verification against multiple keys lands. Today only `[0]` is used.
pub fn load_or_generate(service: &ServiceConfig) -> miette::Result<Key> {
    let files = service.cookie_files()?;
    let active = files
        .first()
        .ok_or_else(|| miette::miette!("cookie_paths resolved empty"))?;
    load_or_generate_at(active)
}

/// Load (or generate) the cookie key at an explicit path — split out so it is
/// testable with a tempdir (no `state_dir()` env dependency).
pub fn load_or_generate_at(path: &Path) -> miette::Result<Key> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() == KEY_LEN => {
            let arr: [u8; KEY_LEN] = bytes
                .as_slice()
                .try_into()
                .expect("length checked in guard");
            Ok(Key::from(&arr))
        }
        Ok(bytes) => Err(miette::miette!(
            "cookie key {} is {} bytes; expected {KEY_LEN}",
            path.display(),
            bytes.len()
        )),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let bytes = fresh_bytes()?;
            crate::keyset::write_secret(path, &bytes)?;
            tracing::info!(path = %path.display(), "generated cookie key on first run");
            Ok(Key::from(&bytes))
        }
        Err(e) => Err(miette::miette!("read cookie key {}: {e}", path.display())),
    }
}

fn fresh_bytes() -> miette::Result<[u8; KEY_LEN]> {
    let mut bytes = [0u8; KEY_LEN];
    getrandom::getrandom(&mut bytes).map_err(|e| miette::miette!("cookie key RNG: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_on_first_run_then_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.key");
        let k1 = load_or_generate_at(&path).unwrap();
        assert!(path.exists());
        // Reload yields a key usable from the same bytes (no regeneration).
        let k2 = load_or_generate_at(&path).unwrap();
        // Key doesn't expose bytes, but re-loading the same file is the contract;
        // a wrong-length file is rejected below.
        let _ = (k1, k2);
    }

    #[test]
    fn rejects_wrong_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.key");
        fs::write(&path, b"too short").unwrap();
        assert!(load_or_generate_at(&path).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn generated_key_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.key");
        load_or_generate_at(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
