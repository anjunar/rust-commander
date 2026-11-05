use std::path::Path;

use anyhow::{Context, Result};
use rust_i18n::t;

use crate::{
    config::ThemePreference,
    ui::{file_viewer_code::open_highlighted_text, file_viewer_common::read_text_file_limited},
};

const MAX_PREVIEW_ROWS: usize = 1_000;
const MAX_PREVIEW_COLS: usize = 40;

pub(crate) fn is_csv_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("csv" | "tsv")
    )
}

pub(crate) fn open(
    parent: &gtk::ApplicationWindow,
    path: &Path,
    max_bytes: u64,
    theme: ThemePreference,
) -> Result<()> {
    let source = read_text_file_limited(path, max_bytes)?;
    let delimiter = if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("tsv"))
    {
        b'\t'
    } else {
        b','
    };

    let table = format_csv_preview(&source, delimiter)?;
    open_highlighted_text(
        parent,
        path,
        &table,
        None,
        &t!("viewer.mode_csv").into_owned(),
        theme,
    )
}

fn format_csv_preview(source: &str, delimiter: u8) -> Result<String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(false)
        .from_reader(source.as_bytes());

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut truncated = false;
    for result in reader.records() {
        if rows.len() >= MAX_PREVIEW_ROWS {
            truncated = true;
            break;
        }
        let record = result.context(t!("viewer.csv_parse_error").into_owned())?;
        let mut row: Vec<String> = record.iter().map(|cell| cell.to_string()).collect();
        if row.len() > MAX_PREVIEW_COLS {
            row.truncate(MAX_PREVIEW_COLS);
            row.push("…".to_string());
        }
        rows.push(row);
    }

    if rows.is_empty() {
        return Ok(String::new());
    }

    let col_count = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; col_count];
    for row in &rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(display_width(cell));
        }
    }

    let mut out = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        for (idx, width) in widths.iter().enumerate() {
            if idx > 0 {
                out.push_str("  ");
            }
            let cell = row.get(idx).map(String::as_str).unwrap_or("");
            out.push_str(&format!("{cell:<width$}"));
        }
        out.push('\n');
        if row_idx == 0 && rows.len() > 1 {
            for (idx, width) in widths.iter().enumerate() {
                if idx > 0 {
                    out.push_str("  ");
                }
                out.push_str(&"-".repeat(*width));
            }
            out.push('\n');
        }
    }

    if truncated {
        out.push_str(&format!(
            "\n… ({})\n",
            t!("viewer.csv_truncated", rows = MAX_PREVIEW_ROWS)
        ));
    }

    Ok(out)
}

fn display_width(value: &str) -> usize {
    value.chars().count().max(1)
}
