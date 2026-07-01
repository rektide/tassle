//! The client-auth keyset (ES256, `private_key_jwt`): load ordered key files
//! under `state_dir()/keyset/`, generating the active signer on first run, and
//! merge them into one [`Keyset`].
//!
//! This is the **client-auth keyset** — one ES256 key for the whole deployment,
//! used to sign the `client_assertion` JWT (`private_key_jwt`). It is distinct
//! from the per-session **DPoP key**, which jacquard generates fresh inside
//! `par()` and stores in `ClientSessionData` — nothing for us to manage. See
//! `doc/axum.md` ("The two keys").
//!
//! The private `d` parameter never reaches a cookie or a log: only its public
//! half is published (via `client_metadata_handler` → `atproto_client_metadata`
//! → `Keyset::public_jwks`, which strips `d`).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use jacquard::oauth::keyset::Keyset;
use tass_config::OAuthConfig;

/// Load the client-auth keyset for an OAuth config.
///
/// Returns `Ok(None)` for a public/loopback client (`!is_confidential()` — no
/// keyset; `token_endpoint_auth_method = none`). For a confidential client,
/// delegates to [`load_merged`] on the resolved [`OAuthConfig::keyset_files`].
pub fn load_or_generate(oauth: &OAuthConfig) -> miette::Result<Option<Keyset>> {
    if !oauth.is_confidential() {
        return Ok(None);
    }
    Ok(Some(load_merged(&oauth.keyset_files()?)?))
}

/// Load and merge ordered keyset files into one [`Keyset`], generating the
/// active signer (`files[0]`) on first run.
///
/// `files[0]` is the active signer; the rest are validators (rotation: prepend
/// a new key file, restart, drain, remove the old). All keys are merged into a
/// single `Keyset`; jacquard's `find_key` picks the signer by algorithm
/// preference then first-in-set, so with a single-algo (ES256) set `[0]` signs.
///
/// Split from [`load_or_generate`] so it is testable with explicit tempdir
/// paths (no `state_dir()` env dependency).
pub fn load_merged(files: &[PathBuf]) -> miette::Result<Keyset> {
    let active = files
        .first()
        .ok_or_else(|| miette::miette!("keyset_paths resolved empty (is_confidential but no files)"))?;
    let mut merged: Vec<_> = Vec::new();
    for path in files {
        let keyset = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Keyset>(&bytes)
                .map_err(|e| miette::miette!("parse keyset {}: {e}", path.display()))?,
            Err(e) if e.kind() == ErrorKind::NotFound && path == active => {
                let kid = kid_for(path);
                let ks = Keyset::generate_es256(kid)
                    .map_err(|e| miette::miette!("generate keyset {}: {e}", path.display()))?;
                let bytes = serde_json::to_vec_pretty(&ks)
                    .map_err(|e| miette::miette!("serialize keyset {}: {e}", path.display()))?;
                write_secret(path, &bytes)?;
                tracing::info!(path = %path.display(), "generated client-auth keyset on first run");
                ks
            }
            Err(e) => {
                return Err(miette::miette!("read keyset {}: {e}", path.display()))
            }
        };
        merged.extend(Vec::from(keyset));
    }
    Keyset::try_from(merged).map_err(|e| miette::miette!("merged keyset invalid: {e}"))
}

/// The `kid` for a generated key: the file stem (e.g. `current`, `2026-07`),
/// so a rotated set's kids read as the operator-named files.
fn kid_for(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "tass-key".to_string())
}

/// Write secret bytes then restrict to 0600 on Unix.
pub(crate) fn write_secret(path: &Path, bytes: &[u8]) -> miette::Result<()> {
    fs::write(path, bytes)
        .map_err(|e| miette::miette!("write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| miette::miette!("chmod 0600 {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_on_first_run_then_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("current.json");
        let ks = load_merged(std::slice::from_ref(&current)).unwrap();
        assert!(current.exists(), "active key file created on first run");
        assert_eq!(ks.public_jwks().keys.len(), 1);
        // kid is the file stem.
        assert_eq!(ks.public_jwks().keys[0].prm.kid.as_deref(), Some("current"));

        // Reload: same key (deterministic load, no regeneration).
        let ks2 = load_merged(&[current]).unwrap();
        assert_eq!(
            ks2.public_jwks().keys[0].prm.kid.as_deref(),
            Some("current"),
        );
    }

    #[test]
    fn multiple_files_merge_with_first_as_signer() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("current.json");
        let b = dir.path().join("next.json");
        // Generate both by loading each as "active" in isolation.
        load_merged(std::slice::from_ref(&a)).unwrap();
        load_merged(std::slice::from_ref(&b)).unwrap();
        // Now load both together: first run generates neither (both exist); they merge.
        let merged = load_merged(&[a, b]).unwrap();
        assert_eq!(merged.public_jwks().keys.len(), 2);
    }

    #[test]
    #[cfg(unix)]
    fn generated_key_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("current.json");
        load_merged(std::slice::from_ref(&f)).unwrap();
        let mode = fs::metadata(&f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
