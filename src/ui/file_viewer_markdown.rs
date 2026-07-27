use std::path::Path;

use gtk::{pango, prelude::*};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

pub(crate) fn is_markdown_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "mdown" | "mkd" | "mdx")
    )
}

pub(crate) fn render_markdown_into_buffer(buffer: &gtk::TextBuffer, markdown: &str) {
    ensure_markdown_tags(buffer);
    buffer.set_text("");

    let mut end = buffer.end_iter();
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(markdown, options);

    let mut style_stack: Vec<&'static str> = Vec::new();
    let mut list_stack: Vec<ListState> = Vec::new();
    let mut pending_list_prefix = String::new();
    let mut link_url: Option<String> = None;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    style_stack.push(heading_tag(level));
                }
                Tag::Paragraph => {}
                Tag::BlockQuote(_) => {
                    style_stack.push("md-quote");
                    insert_styled(buffer, &mut end, "> ", &style_stack);
                }
                Tag::CodeBlock(kind) => {
                    style_stack.push("md-code-block");
                    if let CodeBlockKind::Fenced(lang) = kind {
                        if !lang.is_empty() {
                            insert_styled(buffer, &mut end, &format!("```{lang}\n"), &style_stack);
                        }
                    }
                }
                Tag::List(start) => {
                    list_stack.push(ListState { next_index: start });
                }
                Tag::Item => {
                    pending_list_prefix = list_item_prefix(&list_stack);
                    if let Some(state) = list_stack.last_mut() {
                        if let Some(index) = state.next_index.as_mut() {
                            *index += 1;
                        }
                    }
                    style_stack.push("md-list");
                }
                Tag::Emphasis => style_stack.push("md-emphasis"),
                Tag::Strong => style_stack.push("md-strong"),
                Tag::Strikethrough => style_stack.push("md-strike"),
                Tag::Link { dest_url, .. } => {
                    style_stack.push("md-link");
                    link_url = Some(dest_url.to_string());
                }
                Tag::Image { dest_url, .. } => {
                    insert_styled(
                        buffer,
                        &mut end,
                        &format!("[image: {dest_url}]"),
                        &["md-link"],
                    );
                }
                Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
                Tag::HtmlBlock => style_stack.push("md-code"),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    style_stack.pop();
                    insert_styled(buffer, &mut end, "\n\n", &style_stack);
                }
                TagEnd::Paragraph => {
                    insert_styled(buffer, &mut end, "\n\n", &style_stack);
                }
                TagEnd::BlockQuote(_) => {
                    style_stack.pop();
                    insert_styled(buffer, &mut end, "\n", &style_stack);
                }
                TagEnd::CodeBlock => {
                    insert_styled(buffer, &mut end, "\n", &style_stack);
                    style_stack.pop();
                    insert_styled(buffer, &mut end, "\n", &style_stack);
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    insert_styled(buffer, &mut end, "\n", &style_stack);
                }
                TagEnd::Item => {
                    style_stack.pop();
                    insert_styled(buffer, &mut end, "\n", &style_stack);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::Link => {
                    if let Some(url) = link_url.take() {
                        insert_styled(buffer, &mut end, &format!(" ({url})"), &style_stack);
                    }
                    style_stack.pop();
                }
                TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow => {
                    insert_styled(buffer, &mut end, "\n", &style_stack);
                }
                TagEnd::TableCell => {
                    insert_styled(buffer, &mut end, " | ", &style_stack);
                }
                TagEnd::HtmlBlock => {
                    style_stack.pop();
                }
                _ => {}
            },
            Event::Text(text) => {
                flush_list_prefix(buffer, &mut end, &mut pending_list_prefix, &style_stack);
                insert_styled(buffer, &mut end, &text, &style_stack);
            }
            Event::Code(code) => {
                flush_list_prefix(buffer, &mut end, &mut pending_list_prefix, &style_stack);
                let mut styles = style_stack.clone();
                styles.push("md-code");
                insert_styled(buffer, &mut end, &code, &styles);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                insert_styled(buffer, &mut end, &html, &["md-code"]);
            }
            Event::SoftBreak | Event::HardBreak => {
                insert_styled(buffer, &mut end, "\n", &style_stack);
            }
            Event::Rule => {
                insert_styled(
                    buffer,
                    &mut end,
                    "────────────────────────────────────────\n\n",
                    &[],
                );
            }
            Event::TaskListMarker(checked) => {
                flush_list_prefix(buffer, &mut end, &mut pending_list_prefix, &style_stack);
                let marker = if checked { "[x] " } else { "[ ] " };
                insert_styled(buffer, &mut end, marker, &style_stack);
            }
            Event::InlineMath(math) | Event::DisplayMath(math) => {
                insert_styled(buffer, &mut end, &math, &["md-code"]);
            }
            Event::FootnoteReference(reference) => {
                insert_styled(buffer, &mut end, &format!("[{reference}]"), &["md-link"]);
            }
        }
    }
}

struct ListState {
    next_index: Option<u64>,
}

fn list_item_prefix(list_stack: &[ListState]) -> String {
    let depth = list_stack.len().saturating_sub(1);
    let indent = "  ".repeat(depth);
    match list_stack.last().and_then(|state| state.next_index) {
        Some(index) => format!("{indent}{index}. "),
        None => format!("{indent}• "),
    }
}

fn heading_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "md-h1",
        HeadingLevel::H2 => "md-h2",
        HeadingLevel::H3 => "md-h3",
        _ => "md-h4",
    }
}

fn flush_list_prefix(
    buffer: &gtk::TextBuffer,
    end: &mut gtk::TextIter,
    pending_list_prefix: &mut String,
    styles: &[&str],
) {
    if pending_list_prefix.is_empty() {
        return;
    }
    insert_styled(buffer, end, pending_list_prefix, styles);
    pending_list_prefix.clear();
}

fn insert_styled(buffer: &gtk::TextBuffer, end: &mut gtk::TextIter, text: &str, styles: &[&str]) {
    if text.is_empty() {
        return;
    }

    let start_offset = end.offset();
    buffer.insert(end, text);
    if styles.is_empty() {
        return;
    }

    let start = buffer.iter_at_offset(start_offset);
    let stop = buffer.iter_at_offset(end.offset());
    for style in styles {
        if let Some(tag) = buffer.tag_table().lookup(style) {
            buffer.apply_tag(&tag, &start, &stop);
        }
    }
}

fn ensure_markdown_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();
    if table.lookup("md-h1").is_some() {
        return;
    }

    add_tag(&table, "md-h1", |tag| {
        tag.set_weight(600);
        tag.set_scale(1.55);
    });
    add_tag(&table, "md-h2", |tag| {
        tag.set_weight(600);
        tag.set_scale(1.35);
    });
    add_tag(&table, "md-h3", |tag| {
        tag.set_weight(600);
        tag.set_scale(1.2);
    });
    add_tag(&table, "md-h4", |tag| {
        tag.set_weight(600);
        tag.set_scale(1.1);
    });
    add_tag(&table, "md-strong", |tag| {
        tag.set_weight(700);
    });
    add_tag(&table, "md-emphasis", |tag| {
        tag.set_style(pango::Style::Italic);
    });
    add_tag(&table, "md-strike", |tag| {
        tag.set_strikethrough(true);
    });
    add_tag(&table, "md-code", |tag| {
        tag.set_family(Some("monospace"));
    });
    add_tag(&table, "md-code-block", |tag| {
        tag.set_family(Some("monospace"));
    });
    add_tag(&table, "md-link", |tag| {
        tag.set_underline(pango::Underline::Single);
    });
    add_tag(&table, "md-quote", |tag| {
        tag.set_style(pango::Style::Italic);
    });
    add_tag(&table, "md-list", |_| {});
}

fn add_tag(table: &gtk::TextTagTable, name: &str, configure: impl FnOnce(&gtk::TextTag)) {
    let tag = gtk::TextTag::new(Some(name));
    configure(&tag);
    table.add(&tag);
}

#[cfg(test)]
mod tests {
    use super::is_markdown_path;
    use std::path::Path;

    #[test]
    fn detects_common_markdown_extensions() {
        assert!(is_markdown_path(Path::new("README.md")));
        assert!(is_markdown_path(Path::new("notes.MD")));
        assert!(is_markdown_path(Path::new("doc.markdown")));
        assert!(!is_markdown_path(Path::new("main.rs")));
        assert!(!is_markdown_path(Path::new("README")));
    }
}
