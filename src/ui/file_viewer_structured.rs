use std::path::Path;

use anyhow::{Context, Result};
use rust_i18n::t;
use serde_json::Value as JsonValue;

use crate::{
    config::ThemePreference,
    ui::{file_viewer_code::open_highlighted_text, file_viewer_common::read_text_file_limited},
};

#[derive(Clone, Copy)]
pub(crate) enum StructuredKind {
    Json,
    Yaml,
    Toml,
    Xml,
}

pub(crate) fn detect_structured_kind(path: &Path) -> Option<StructuredKind> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("json" | "jsonc") => Some(StructuredKind::Json),
        Some("yaml" | "yml") => Some(StructuredKind::Yaml),
        Some("toml") => Some(StructuredKind::Toml),
        Some("xml") => Some(StructuredKind::Xml),
        _ => None,
    }
}

pub(crate) fn open(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    kind: StructuredKind,
    max_bytes: u64,
    theme: ThemePreference,
) -> Result<()> {
    let source = read_text_file_limited(path, max_bytes)?;
    let (text, language_id, mode) = match kind {
        StructuredKind::Json => (
            pretty_json(&source)?,
            "json",
            t!("viewer.mode_json").into_owned(),
        ),
        StructuredKind::Yaml => (
            pretty_yaml(&source)?,
            "yaml",
            t!("viewer.mode_yaml").into_owned(),
        ),
        StructuredKind::Toml => (
            pretty_toml(&source)?,
            "toml",
            t!("viewer.mode_toml").into_owned(),
        ),
        StructuredKind::Xml => (source, "xml", t!("viewer.mode_xml").into_owned()),
    };

    open_highlighted_text(parent, path, &text, Some(language_id), &mode, theme)
}

fn pretty_json(source: &str) -> Result<String> {
    let value: JsonValue =
        serde_json::from_str(source).context(t!("viewer.structured_parse_error").into_owned())?;
    serde_json::to_string_pretty(&value).context(t!("viewer.structured_parse_error").into_owned())
}

fn pretty_yaml(source: &str) -> Result<String> {
    let value: serde_yml::Value =
        serde_yml::from_str(source).context(t!("viewer.structured_parse_error").into_owned())?;
    serde_yml::to_string(&value).context(t!("viewer.structured_parse_error").into_owned())
}

fn pretty_toml(source: &str) -> Result<String> {
    let value: toml::Value =
        toml::from_str(source).context(t!("viewer.structured_parse_error").into_owned())?;
    toml::to_string_pretty(&value).context(t!("viewer.structured_parse_error").into_owned())
}
