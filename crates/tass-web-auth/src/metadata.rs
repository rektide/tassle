//! Derive jacquard's [`ClientData`] (keyset + [`AtprotoClientMetadata`]) from a
//! [`ServiceConfig`]. Confidential vs loopback is **emergent** from
//! [`OAuthConfig::is_confidential`] + the `public_url` scheme — never a mode
//! flag (DPoP is hardcoded `true` by `atproto_client_metadata` either way).
//!
//! The confidential `client_id` is the public metadata URL, so changing the
//! hostname re-keys every session. Confirm the origin before first run.

use jacquard::common::deps::{fluent_uri::Uri, smol_str::SmolStr};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::keyset::Keyset;
use jacquard::oauth::scopes::Scopes;
use jacquard::oauth::session::ClientData;
use tass_config::ServiceConfig;

/// The path jacquard-axum mounts the callback at (`OAuthWebConfig` default).
/// `redirect_uris` derive from `public_url + this`, so they cannot drift from
/// the mounted route.
const CALLBACK_PATH: &str = "/oauth/callback";
/// The fixed path `client_metadata_handler` serves — the confidential
/// `client_id` is `public_url + this`.
const METADATA_PATH: &str = "/oauth-client-metadata.json";

/// Build the [`ClientData`] for a service: client metadata + optional keyset.
///
/// Confidential (`is_confidential()` + `https://` `public_url`) builds hosted
/// metadata with `private_key_jwt`; otherwise loopback (`http://localhost`,
/// `none`) — the atproto dev exception. The keyset is attached only when
/// present, so `atproto_client_metadata` selects the right auth method.
pub fn client_data(
    service: &ServiceConfig,
    keyset: Option<Keyset>,
) -> miette::Result<ClientData<SmolStr>> {
    let scopes = scopes(service)?;
    let confidential = service.oauth.is_confidential()
        && service
            .public_url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://"));
    let config = if confidential {
        confidential_metadata(service, scopes)?
    } else {
        loopback_metadata(service, scopes)?
    };
    Ok(if keyset.is_some() {
        ClientData::new(keyset, config)
    } else {
        ClientData::new_public(config)
    })
}

fn scopes(service: &ServiceConfig) -> miette::Result<Scopes<SmolStr>> {
    Scopes::new(SmolStr::from(service.oauth.scopes.join(" ")))
        .map_err(|e| miette::miette!("invalid scopes {:?}: {e}", service.oauth.scopes))
}

fn confidential_metadata(
    service: &ServiceConfig,
    scopes: Scopes<SmolStr>,
) -> miette::Result<AtprotoClientMetadata<SmolStr>> {
    let base = service
        .public_url
        .as_ref()
        .map(|u| u.trim_end_matches('/').to_string())
        .filter(|u| u.starts_with("https://"))
        .ok_or_else(|| miette::miette!("confidential client needs an https:// public_url"))?;
    let redirect_uri = parse_uri(&format!("{base}{CALLBACK_PATH}"))?;
    let client_id = parse_uri(&format!("{base}{METADATA_PATH}"))?;
    let mut md = AtprotoClientMetadata::new(vec![redirect_uri], client_id, Some(scopes));
    if let Some(name) = service.oauth.client_name.clone() {
        md = md.with_prod_info(
            SmolStr::from(name),
            service.logo_uri().and_then(|u| parse_uri(&u).ok()),
            service.tos_uri().and_then(|u| parse_uri(&u).ok()),
            service.privacy_uri().and_then(|u| parse_uri(&u).ok()),
        );
    }
    Ok(md)
}

fn loopback_metadata(
    service: &ServiceConfig,
    scopes: Scopes<SmolStr>,
) -> miette::Result<AtprotoClientMetadata<SmolStr>> {
    let port = service.bind.map(|a| a.port()).unwrap_or(3000);
    let redirect_uris = vec![
        parse_uri(&format!("http://127.0.0.1:{port}{CALLBACK_PATH}"))?,
        parse_uri(&format!("http://[::1]:{port}{CALLBACK_PATH}"))?,
    ];
    Ok(AtprotoClientMetadata::new_localhost(
        Some(redirect_uris),
        Some(scopes),
    ))
}

fn parse_uri(s: &str) -> miette::Result<Uri<String>> {
    Uri::parse(s.to_string()).map_err(|(e, _)| miette::miette!("invalid URI {s:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tass_config::{OAuthConfig, ServiceConfig};

    fn confidential_service() -> ServiceConfig {
        ServiceConfig {
            bind: Some("127.0.0.1:3000".parse().unwrap()),
            public_url: Some("https://telluri.at".into()),
            cookie_paths: vec!["current".into()],
            oauth: OAuthConfig {
                scopes: vec!["atproto".into()],
                keyset_paths: vec!["current".into()],
                client_name: Some("telluri.at".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn confidential_metadata_derives_client_id_and_redirect() {
        let svc = confidential_service();
        let cd = client_data(&svc, None).unwrap();
        assert!(cd.keyset.is_none(), "no keyset passed yet");
        assert_eq!(
            cd.config.client_id.as_str(),
            "https://telluri.at/oauth-client-metadata.json",
        );
        assert_eq!(
            cd.config.redirect_uris[0].as_str(),
            "https://telluri.at/oauth/callback",
        );
        assert_eq!(cd.config.client_name.as_deref(), Some("telluri.at"));
    }

    #[test]
    fn loopback_metadata_when_no_keyset() {
        let svc = ServiceConfig {
            bind: Some("127.0.0.1:4567".parse().unwrap()),
            public_url: None,
            oauth: OAuthConfig {
                keyset_paths: vec![],
                scopes: vec!["atproto".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let cd = client_data(&svc, None).unwrap();
        // loopback client_id encodes the redirect URIs + scope in the query.
        let cid = cd.config.client_id.as_str();
        assert!(cid.starts_with("http://localhost/"), "got {cid}");
        assert!(cd.config.redirect_uris.iter().any(|u| u
            .as_str()
            .starts_with("http://127.0.0.1:4567/oauth/callback")));
    }

    #[test]
    fn https_without_keyset_still_loopback() {
        // public_url is https but no keyset → not confidential (emergent).
        let svc = ServiceConfig {
            public_url: Some("https://telluri.at".into()),
            oauth: OAuthConfig {
                keyset_paths: vec![],
                ..Default::default()
            },
            ..Default::default()
        };
        let cd = client_data(&svc, None).unwrap();
        assert!(
            cd.config.client_id.as_str().starts_with("http://localhost/"),
            "no keyset ⇒ loopback even with https public_url"
        );
    }
}
