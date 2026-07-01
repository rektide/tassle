//! Generic atproto record access — identity resolution + record reads.
//!
//! The substrate every tassle domain shares to get records in and out of a
//! PDS: resolve a DID/handle to its PDS, point a client at it, and read records
//! back as a normalized [`RecordEnvelope`]. Deliberately **generic** — it deals
//! in `Nsid`/`rkey`/`serde_json::Value` and knows nothing about any specific
//! lexicon (no `actor.rpg.stats`, no mage). That bright line is what keeps it a
//! reusable library rather than a grab-bag: domain meaning lives in the domain
//! crates (`tass-mage`, …) that interpret the `value` this hands back.
//!
//! Generic over the caller's jacquard client: pass a `BasicClient` for public
//! reads today, or an authenticated session later — [`resolve`] needs
//! [`IdentityResolver`], the read fns need [`XrpcClient`], and a `BasicClient`
//! is both.

use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{Nsid, RecordKey};
use jacquard_common::xrpc::atproto::{GetRecord, GetRecordError, ListRecords};
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use serde_json::Value;

pub type Result<T> = std::result::Result<T, RepoError>;

/// Errors from record access. Hand-rolled (no thiserror) to keep the dependency
/// surface minimal, matching `tass-repo-mage` / `tass-ledger`. Transport and
/// decode errors are stringified so this crate stays generic over the caller's
/// client without leaking its error type parameters.
#[derive(Debug)]
pub enum RepoError {
    /// A DID/handle/collection/rkey string failed AT-Proto syntax validation.
    Ident(String),
    /// Resolving the identity to a PDS failed.
    Resolve(String),
    /// A resolved PDS endpoint was not a valid URI.
    Uri(String),
    /// A jacquard XRPC transport/typed error.
    Xrpc(String),
    /// Decoding a record body to JSON failed.
    Decode(String),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::Ident(e) => write!(f, "invalid identifier: {e}"),
            RepoError::Resolve(e) => write!(f, "identity resolution failed: {e}"),
            RepoError::Uri(e) => write!(f, "resolved PDS endpoint is not a valid URI: {e}"),
            RepoError::Xrpc(e) => write!(f, "xrpc error: {e}"),
            RepoError::Decode(e) => write!(f, "record decode error: {e}"),
        }
    }
}

impl std::error::Error for RepoError {}

/// A DID resolved to the PDS that hosts its repo.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The canonical DID (handles are resolved through to their DID).
    pub did: AtIdentifier,
    /// The actor's PDS service endpoint.
    pub pds: String,
}

/// The last path segment of an AT-URI (its record key).
pub fn rkey_from_uri(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or(uri)
}

/// Resolve a DID or handle string to its canonical DID + PDS endpoint.
///
/// Handles are resolved through to their DID, so [`Resolved::did`] is always a
/// DID regardless of the input form.
pub async fn resolve<C>(client: &C, actor: &str) -> Result<Resolved>
where
    C: IdentityResolver + Sync + ?Sized,
{
    let ident = AtIdentifier::new_owned(actor).map_err(|e| RepoError::Ident(e.to_string()))?;
    match ident {
        AtIdentifier::Did(did) => {
            let pds = client
                .pds_for_did(&did)
                .await
                .map_err(|e| RepoError::Resolve(e.to_string()))?;
            Ok(Resolved {
                did: AtIdentifier::Did(did),
                pds: pds.to_string(),
            })
        }
        AtIdentifier::Handle(handle) => {
            let (did, pds) = client
                .pds_for_handle(&handle)
                .await
                .map_err(|e| RepoError::Resolve(e.to_string()))?;
            Ok(Resolved {
                did: AtIdentifier::Did(did),
                pds: pds.to_string(),
            })
        }
    }
}

/// [`resolve`] the actor, then point `client`'s `base_uri` at the resolved PDS
/// so subsequent reads hit the right server. Returns the [`Resolved`] identity.
pub async fn resolve_and_point<C>(client: &C, actor: &str) -> Result<Resolved>
where
    C: IdentityResolver + XrpcClient + Sync + ?Sized,
{
    let resolved = resolve(client, actor).await?;
    let uri = Uri::parse(resolved.pds.clone())
        .map_err(|_| RepoError::Uri(resolved.pds.clone()))?
        .to_owned();
    client.set_base_uri(uri).await;
    Ok(resolved)
}

/// A record read back from a repo, normalized to plain JSON. Domain crates
/// interpret [`value`](Self::value); this crate stays lexicon-agnostic.
#[derive(Debug, Clone)]
pub struct RecordEnvelope {
    pub uri: String,
    pub cid: Option<String>,
    pub rkey: String,
    pub value: Value,
}

/// One page of a `listRecords` response.
#[derive(Debug, Clone)]
pub struct ListPage {
    pub cursor: Option<String>,
    pub records: Vec<RecordEnvelope>,
}

/// `getRecord` for `repo`/`collection`/`rkey`. Returns `Ok(None)` for a
/// not-found record (rather than an error). The client must already be pointed
/// at the actor's PDS (see [`resolve_and_point`]).
pub async fn get_record<C>(
    client: &C,
    repo: AtIdentifier,
    collection: &str,
    rkey: &str,
) -> Result<Option<RecordEnvelope>>
where
    C: XrpcClient + Sync + ?Sized,
{
    let request = GetRecord {
        repo,
        collection: Nsid::new_owned(collection).map_err(|e| RepoError::Ident(e.to_string()))?,
        rkey: RecordKey::any_owned(rkey).map_err(|e| RepoError::Ident(e.to_string()))?,
        cid: None,
    };
    let response = client
        .send(request)
        .await
        .map_err(|e| RepoError::Xrpc(e.to_string()))?;
    match response.into_output() {
        Ok(output) => {
            let uri = output.uri.as_str().to_owned();
            Ok(Some(RecordEnvelope {
                rkey: rkey_from_uri(&uri).to_owned(),
                cid: output.cid.map(|c| c.as_str().to_owned()),
                value: serde_json::to_value(&output.value)
                    .map_err(|e| RepoError::Decode(e.to_string()))?,
                uri,
            }))
        }
        Err(XrpcError::Xrpc(GetRecordError::RecordNotFound(_))) => Ok(None),
        Err(e) => Err(RepoError::Xrpc(e.to_string())),
    }
}

/// `listRecords` for a collection. The client must already be pointed at the
/// actor's PDS (see [`resolve_and_point`]).
pub async fn list_records<C>(
    client: &C,
    repo: AtIdentifier,
    collection: &str,
    limit: Option<i64>,
    cursor: Option<String>,
    reverse: bool,
) -> Result<ListPage>
where
    C: XrpcClient + Sync + ?Sized,
{
    let request = ListRecords {
        repo,
        collection: Nsid::new_owned(collection).map_err(|e| RepoError::Ident(e.to_string()))?,
        cursor: cursor.map(Into::into),
        limit,
        reverse: if reverse { Some(true) } else { None },
    };
    let response = client
        .send(request)
        .await
        .map_err(|e| RepoError::Xrpc(e.to_string()))?;
    let output = response
        .into_output()
        .map_err(|e| RepoError::Decode(e.to_string()))?;
    let records = output
        .records
        .into_iter()
        .map(|record| {
            let uri = record.uri.as_str().to_owned();
            Ok(RecordEnvelope {
                rkey: rkey_from_uri(&uri).to_owned(),
                cid: record.cid.map(|c| c.as_str().to_owned()),
                value: serde_json::to_value(&record.value)
                    .map_err(|e| RepoError::Decode(e.to_string()))?,
                uri,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ListPage {
        cursor: output.cursor.map(|c| c.to_string()),
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rkey_is_last_at_uri_segment() {
        assert_eq!(
            rkey_from_uri("at://did:plc:abc/actor.rpg.stats/mage"),
            "mage"
        );
        assert_eq!(rkey_from_uri("mage"), "mage");
        assert_eq!(rkey_from_uri("at://did:plc:abc/coll/self"), "self");
    }
}
