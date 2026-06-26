use chrono::Utc;
use miette::IntoDiagnostic;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use toml_edit::{value, DocumentMut, Item, Table};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub did: String,
    pub handle: Option<String>,
    pub pds: String,
    pub active: bool,
    pub path: PathBuf,
    pub updated_at: Option<String>,
}

pub fn tassle_config_dir() -> miette::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir).join("tassle"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| miette::miette!("HOME is unset; cannot resolve XDG config directory"))?;
    Ok(PathBuf::from(home).join(".config").join("tassle"))
}

pub fn profile_dir() -> miette::Result<PathBuf> {
    Ok(tassle_config_dir()?.join("config.toml.d"))
}

pub fn profile_path(did: &str) -> miette::Result<PathBuf> {
    Ok(profile_dir()?.join(format!("{did}.toml")))
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn read_document(path: &PathBuf) -> miette::Result<DocumentMut> {
    if path.exists() {
        let text = fs::read_to_string(path).into_diagnostic()?;
        Ok(text.parse::<DocumentMut>().into_diagnostic()?)
    } else {
        let text = "# tassle profile fragment\n# Managed by `tassle auth login`; safe to extend with profile-specific settings.\n\n";
        Ok(text.parse::<DocumentMut>().into_diagnostic()?)
    }
}

fn item_string(doc: &DocumentMut, key: &str) -> Option<String> {
    doc.get(key)?.as_str().map(ToOwned::to_owned)
}

fn item_bool(doc: &DocumentMut, key: &str) -> Option<bool> {
    doc.get(key)?.as_bool()
}

fn profile_from_doc(path: PathBuf, doc: &DocumentMut) -> Option<Profile> {
    let did = item_string(doc, "did")?;
    Some(Profile {
        id: item_string(doc, "id").unwrap_or_else(|| did.clone()),
        did,
        handle: item_string(doc, "handle"),
        pds: item_string(doc, "pds")?,
        active: item_bool(doc, "active").unwrap_or(false),
        updated_at: item_string(doc, "updated_at"),
        path,
    })
}

pub fn save_profile(did: &str, handle: Option<&str>, pds: &str) -> miette::Result<Profile> {
    let dir = profile_dir()?;
    fs::create_dir_all(&dir).into_diagnostic()?;
    let path = profile_path(did)?;
    let mut doc = read_document(&path)?;

    doc["id"] = value(did);
    doc["did"] = value(did);
    if let Some(handle) = handle {
        doc["handle"] = value(handle);
    }
    doc["pds"] = value(pds);
    doc["active"] = value(true);
    if matches!(doc.get("created_at"), None | Some(Item::None)) {
        doc["created_at"] = value(now());
    }
    doc["updated_at"] = value(now());

    fs::write(&path, doc.to_string()).into_diagnostic()?;
    let doc = read_document(&path)?;
    profile_from_doc(path, &doc).ok_or_else(|| miette::miette!("saved profile is incomplete"))
}

pub fn load_profiles() -> miette::Result<Vec<Profile>> {
    let dir = profile_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();
    for entry in fs::read_dir(dir).into_diagnostic()? {
        let path = entry.into_diagnostic()?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let doc = read_document(&path)?;
        if let Some(profile) = profile_from_doc(path, &doc) {
            profiles.push(profile);
        }
    }
    Ok(profiles)
}

pub fn default_profile() -> miette::Result<Profile> {
    let mut profiles = load_profiles()?;
    profiles.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.did.cmp(&b.did))
    });
    profiles.into_iter().next().ok_or_else(|| {
        miette::miette!(
            "no tassle profile configured; run `tassle auth login <did-or-handle>` first"
        )
    })
}

pub fn default_did() -> miette::Result<String> {
    Ok(default_profile()?.did)
}

fn dotted_path(key: &str) -> miette::Result<Vec<&str>> {
    let parts = key.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        miette::bail!("config key must be a non-empty dotted TOML path");
    }
    Ok(parts)
}

fn get_item<'a>(doc: &'a DocumentMut, parts: &[&str]) -> Option<&'a Item> {
    let mut item = doc.get(parts[0])?;
    for part in &parts[1..] {
        item = item.get(part)?;
    }
    Some(item)
}

fn get_or_create_child<'a>(item: &'a mut Item, key: &str) -> &'a mut Item {
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut()
        .expect("table was just created")
        .entry(key)
        .or_insert(Item::None)
}

fn set_item(doc: &mut DocumentMut, parts: &[&str], item: Item) {
    if parts.len() == 1 {
        doc[parts[0]] = item;
        return;
    }

    let mut cursor = doc
        .entry(parts[0])
        .or_insert_with(|| Item::Table(Table::new()));
    for part in &parts[1..parts.len() - 1] {
        cursor = get_or_create_child(cursor, part);
    }
    let last = parts[parts.len() - 1];
    get_or_create_child(cursor, last);
    if let Some(table) = cursor.as_table_mut() {
        table[last] = item;
    }
}

fn parse_value(input: &str) -> Item {
    if input.eq_ignore_ascii_case("true") {
        return value(true);
    }
    if input.eq_ignore_ascii_case("false") {
        return value(false);
    }
    if let Ok(parsed) = input.parse::<i64>() {
        return value(parsed);
    }
    if let Ok(parsed) = input.parse::<f64>() {
        return value(parsed);
    }
    value(input)
}

pub fn read_profile_value(key: &str) -> miette::Result<(Profile, Option<String>)> {
    let profile = default_profile()?;
    let doc = read_document(&profile.path)?;
    let parts = dotted_path(key)?;
    let value = get_item(&doc, &parts).map(|item| {
        item.as_value()
            .map(ToString::to_string)
            .unwrap_or_else(|| item.to_string().trim().to_owned())
    });
    Ok((profile, value))
}

pub fn write_profile_value(key: &str, value_input: &str) -> miette::Result<(Profile, String)> {
    let profile = default_profile()?;
    let mut doc = read_document(&profile.path)?;
    let parts = dotted_path(key)?;
    let item = parse_value(value_input);
    let rendered = item
        .as_value()
        .map(ToString::to_string)
        .unwrap_or_else(|| item.to_string().trim().to_owned());
    set_item(&mut doc, &parts, item);
    doc["updated_at"] = value(now());
    fs::write(&profile.path, doc.to_string()).into_diagnostic()?;
    let doc = read_document(&profile.path)?;
    let profile = profile_from_doc(profile.path, &doc)
        .ok_or_else(|| miette::miette!("updated profile is incomplete"))?;
    Ok((profile, rendered))
}
