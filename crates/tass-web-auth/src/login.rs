//! The one bit of real UI: a static-ish `GET /login` page.
//!
//! One text input (handle / DID / PDS host) that GETs `/oauth/start`
//! (jacquard-axum's `start_auth_query`), plus a hidden `return_to`. No
//! framework — the atproto spec puts all consent/authentication UI on the
//! user's own Authorization Server; we only collect the identifier and hand off.
//!
//! Override: `tass-web` may mount its own `GET /login` and skip this handler.

use axum::extract::Query;
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

/// Query params accepted by the login page: an optional local `return_to`
/// (validated/safe-rounded by jacquard-axum's `validate_return_to` downstream).
#[derive(Debug, Default, Deserialize)]
pub struct LoginQuery {
    #[serde(default)]
    pub return_to: Option<String>,
}

/// `GET /login` — render the static-ish login page.
pub async fn login_page(Query(query): Query<LoginQuery>) -> Response {
    let return_to = query.return_to.as_deref().unwrap_or("/");
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(render(return_to)),
    )
        .into_response()
}

/// Render the login page HTML, HTML-escaping `return_to` into the hidden
/// input's double-quoted attribute. Exposed for unit tests / alternate handlers.
pub fn render(return_to: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Sign in with AT Protocol</title>
</head>
<body>
  <main>
    <h1>Sign in with AT Protocol</h1>
    <p>Enter your handle, DID, or PDS URL. You will authenticate on your own
    server (PDS).</p>
    <form method="get" action="/oauth/start">
      <p>
        <label for="identifier">Handle, DID, or PDS URL</label><br>
        <input id="identifier" name="identifier" autocomplete="username"
          placeholder="alice.bsky.social" required>
      </p>
      <input type="hidden" name="return_to" value="{return_to}">
      <button type="submit">Sign in</button>
    </form>
  </main>
</body>
</html>"#,
        return_to = html_escape::encode_double_quoted_attribute(return_to),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_escapes_return_to_attribute() {
        let html = render(r#"/x" onmouseover="evil"#);
        // The raw quote/backslash sequences must not survive unescaped into the
        // attribute value.
        assert!(!html.contains(r#"value="/x" onmouseover="evil""#));
        assert!(html.contains(r#"name="return_to""#));
    }

    #[test]
    fn render_defaults_return_to_to_root() {
        let html = render("/");
        assert!(html.contains(r#"value="/""#));
    }
}
