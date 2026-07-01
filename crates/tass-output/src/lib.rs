//! Output layer — one place that renders a value or record as JSON, DAG-CBOR,
//! or a human table, to any writer.
//!
//! Two entry points share the machine formats:
//! - [`render_value`] takes any [`Serialize`] value; `Table` falls back to
//!   pretty JSON. Use it for record types you can't implement a trait on (e.g.
//!   foreign lexicon record structs). This replaces the old `commands::emit`.
//! - [`render`] takes a [`Render`] value, whose [`Render::write_table`] gives a
//!   real human table for the `Table` format. A CLI command implements `Render`
//!   for its own output type and overrides `write_table`; until it does, the
//!   default `write_table` is pretty JSON, identical to `render_value`.
//!
//! The `Render: Serialize` supertrait with a defaulted `write_table` is the
//! seam that lets human tables land incrementally, per command, with no
//! big-bang and no regression: an empty `impl Render for Foo {}` behaves
//! exactly like today's JSON-for-`table` fallback.
//!
//! Human table overrides (a follow-up pass) are slated to use the `tabled`
//! crate for list grids; this pass ships the trait + envelope + machine formats
//! only.

use std::io::Write;

use serde::Serialize;

/// The output format, selected by the CLI's global `--format` flag. Kept free
/// of `clap` so this crate is reusable outside the CLI; the CLI maps its own
/// flag enum onto this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Machine-readable JSON (pretty-printed).
    Json,
    /// DAG-CBOR bytes (atproto wire format).
    Cbor,
    /// Human-readable. Defaults to pretty JSON until a type provides a real
    /// [`Render::write_table`].
    Table,
}

/// Errors from rendering. Hand-rolled (no thiserror/miette) to keep the crate
/// dependency-light and reusable, matching the other `tass-*` crates.
#[derive(Debug)]
pub enum RenderError {
    /// Serializing the value (JSON or DAG-CBOR) failed.
    Encode(String),
    /// Writing to the output sink failed.
    Io(std::io::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Encode(e) => write!(f, "encode error: {e}"),
            RenderError::Io(e) => write!(f, "write error: {e}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenderError::Io(e) => Some(e),
            RenderError::Encode(_) => None,
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self {
        RenderError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, RenderError>;

/// Write pretty JSON + trailing newline (the JSON / default-table form).
fn write_json<T: Serialize + ?Sized>(value: &T, w: &mut dyn Write) -> Result<()> {
    let s = serde_json::to_string_pretty(value).map_err(|e| RenderError::Encode(e.to_string()))?;
    writeln!(w, "{s}")?;
    Ok(())
}

/// Write DAG-CBOR bytes (no trailing newline — it's a binary wire format).
fn write_cbor<T: Serialize + ?Sized>(value: &T, w: &mut dyn Write) -> Result<()> {
    let bytes =
        serde_ipld_dagcbor::to_vec(value).map_err(|e| RenderError::Encode(e.to_string()))?;
    w.write_all(&bytes)?;
    Ok(())
}

/// Render any [`Serialize`] value. `Table` falls back to pretty JSON — records
/// have no inherent tabular form. Byte-for-byte the behaviour of the old
/// `commands::emit`.
pub fn render_value<T: Serialize + ?Sized>(
    value: &T,
    fmt: Format,
    w: &mut dyn Write,
) -> Result<()> {
    match fmt {
        Format::Json | Format::Table => write_json(value, w),
        Format::Cbor => write_cbor(value, w),
    }
}

/// A value a command emits, with an optional human-table form.
///
/// Machine formats come free from [`Serialize`]; the `Table` format calls
/// [`write_table`](Render::write_table), which defaults to pretty JSON. A
/// command overrides `write_table` to render a real table for its output type.
pub trait Render: Serialize {
    /// Human-readable rendering. Default: pretty JSON (same as [`render_value`]
    /// with [`Format::Table`]), so an empty `impl Render` is a no-op change.
    fn write_table(&self, w: &mut dyn Write) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        writeln!(w, "{s}")
    }
}

/// Render a [`Render`] value: `Table` uses its [`Render::write_table`]; the
/// machine formats go through [`render_value`].
pub fn render<T: Render + ?Sized>(value: &T, fmt: Format, w: &mut dyn Write) -> Result<()> {
    match fmt {
        Format::Table => value.write_table(w).map_err(RenderError::Io),
        machine => render_value(value, machine, w),
    }
}

/// A record and its atproto coordinates — the shared shape for both read
/// commands (which fill the coordinates) and generated records (which fill only
/// [`value`](Self::value)). Matches the README record envelope.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<V> {
    /// `at://did/collection/rkey`, when the record has a location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    /// The repo DID (`did:plc:…`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rkey: Option<String>,
    /// The record body (or a normalized projection of it).
    pub value: V,
    /// An optional domain-normalized view alongside the raw `value`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized: Option<serde_json::Value>,
}

impl<V> Envelope<V> {
    /// A value-only envelope (no atproto coordinates) — for generated records.
    pub fn value(value: V) -> Self {
        Envelope {
            uri: None,
            cid: None,
            repo: None,
            collection: None,
            rkey: None,
            value,
            normalized: None,
        }
    }
}

impl<V: Serialize> Render for Envelope<V> {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn to_string<T: Serialize>(v: &T, fmt: Format) -> String {
        let mut buf = Vec::new();
        render_value(v, fmt, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn json_is_pretty_with_trailing_newline() {
        let out = to_string(&json!({ "a": 1 }), Format::Json);
        assert_eq!(out, "{\n  \"a\": 1\n}\n");
    }

    #[test]
    fn table_falls_back_to_json() {
        let v = json!({ "a": 1 });
        assert_eq!(to_string(&v, Format::Table), to_string(&v, Format::Json));
    }

    #[test]
    fn cbor_is_raw_bytes_no_newline() {
        let mut buf = Vec::new();
        render_value(&json!({ "a": 1 }), Format::Cbor, &mut buf).unwrap();
        let expect = serde_ipld_dagcbor::to_vec(&json!({ "a": 1 })).unwrap();
        assert_eq!(buf, expect);
        assert_ne!(buf.last(), Some(&b'\n'));
    }

    #[test]
    fn default_render_matches_render_value() {
        // An empty `impl Render` (via Envelope's default write_table) renders
        // Table identically to the Serialize-only path.
        let env = Envelope::value(json!({ "x": 2 }));
        let mut a = Vec::new();
        render(&env, Format::Table, &mut a).unwrap();
        let mut b = Vec::new();
        render_value(&env, Format::Table, &mut b).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn value_only_envelope_serializes_without_coordinates() {
        let env = Envelope::value(json!({ "x": 2 }));
        let s = serde_json::to_value(&env).unwrap();
        assert_eq!(s, json!({ "value": { "x": 2 } }));
    }
}
