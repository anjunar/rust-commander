use std::path::Path;

use anyhow::{Context, Result};
use htmd::HtmlToMarkdown;

pub(crate) fn is_html_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("html" | "htm" | "xhtml")
    )
}

pub(crate) fn html_to_preview_markdown(html: &str) -> Result<String> {
    let converter = HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "noscript"])
        .build();
    converter
        .convert(html)
        .context("Could not convert HTML to preview")
}

#[cfg(test)]
mod tests {
    use super::{html_to_preview_markdown, is_html_path};
    use std::path::Path;

    #[test]
    fn detects_common_html_extensions() {
        assert!(is_html_path(Path::new("index.html")));
        assert!(is_html_path(Path::new("page.HTM")));
        assert!(is_html_path(Path::new("doc.xhtml")));
        assert!(!is_html_path(Path::new("readme.md")));
        assert!(!is_html_path(Path::new("main.rs")));
    }

    #[test]
    fn converts_basic_html_heading() {
        let markdown = html_to_preview_markdown("<h1>Hello</h1><p>World</p>").unwrap();
        assert!(markdown.contains("# Hello"));
        assert!(markdown.contains("World"));
    }

    #[test]
    fn skips_script_and_style() {
        let markdown = html_to_preview_markdown(
            "<style>body{color:red}</style><p>Visible</p><script>alert(1)</script>",
        )
        .unwrap();
        assert!(markdown.contains("Visible"));
        assert!(!markdown.contains("alert"));
        assert!(!markdown.contains("color:red"));
    }
}
