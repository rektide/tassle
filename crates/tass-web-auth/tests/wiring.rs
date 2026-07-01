//! Wiring smoke test: boot a production [`AppState`] over an in-memory turso
//! store and confirm jacquard-axum's metadata route serves *our* derived
//! `ClientData`. No network — the metadata handler only reads state.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use jacquard::identity::PublicResolver;
use jacquard_axum::oauth::routes;
use jac_stores::{OAuthStore, TursoRepository};
use tass_config::{OAuthConfig, ServiceConfig};
use tass_web_auth::{boot_prod, default_web_config, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn metadata_route_serves_confidential_client_id() {
    let keyset_dir = tempfile::tempdir().unwrap();
    let cookie_dir = tempfile::tempdir().unwrap();
    let service = ServiceConfig {
        bind: Some("127.0.0.1:3000".parse().unwrap()),
        public_url: Some("https://telluri.at".into()),
        cookie_paths: vec![cookie_dir
            .path()
            .join("current.key")
            .to_str()
            .unwrap()
            .to_string()],
        oauth: OAuthConfig {
            scopes: vec!["atproto".into()],
            // absolute + has extension → used verbatim (no state_dir() needed)
            keyset_paths: vec![keyset_dir
                .path()
                .join("current.json")
                .to_str()
                .unwrap()
                .to_string()],
            client_name: Some("telluri.at".into()),
            ..Default::default()
        },
    };

    let repo = TursoRepository::open_local(":memory:").await.unwrap();
    let store = OAuthStore::new(repo);
    let web_config = default_web_config();
    let state = boot_prod(service, store, web_config.clone()).unwrap();

    // keyset was generated on first run + attached → confidential.
    assert!(state.oauth.registry.client_data.keyset.is_some());

    let app = routes::<PublicResolver, OAuthStore<TursoRepository>, AppState>(&web_config)
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth-client-metadata.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["client_id"], "https://telluri.at/oauth-client-metadata.json");
    // confidential ⇒ private_key_jwt + an inline jwks.
    assert_eq!(body["token_endpoint_auth_method"], "private_key_jwt");
    assert!(body["jwks"]["keys"].as_array().is_some_and(|k| !k.is_empty()));
    assert_eq!(body["application_type"], "web");
}
