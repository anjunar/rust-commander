use std::{borrow::Cow, io};

use crate::viewer::{file_source::FileSource, line_index::LineIndex};

const READ_CHUNK_SIZE: usize = 64 * 1024;
const MAX_LINE_PREVIEW_BYTES: usize = 4096;

#[derive(Debug, Clone)]
pub struct TextRender {
    pub lines: Vec<String>,
}

pub fn render_text_lines(
    source: &FileSource,
    line_index: &mut LineIndex,
    first_line: usize,
    visible_lines: usize,
    horizontal_offset: usize,
) -> io::Result<TextRender> {
    if visible_lines == 0 {
        return Ok(TextRender { lines: Vec::new() });
    }

    if source.is_empty() {
        return Ok(TextRender {
            lines: vec![String::new()],
        });
    }

    line_index.ensure_lines(source, first_line.saturating_add(visible_lines))?;

    let mut lines = Vec::with_capacity(visible_lines);
    for line_number in first_line..first_line.saturating_add(visible_lines) {
        let Some(line_start) = line_index.line_start(line_number) else {
            break;
        };
        let next_line_start = line_index.line_start(line_number + 1);
        let preview = read_line_preview(source, line_start, next_line_start, horizontal_offset)?;
        lines.push(preview);
    }

    Ok(TextRender { lines })
}

fn read_line_preview(
    source: &FileSource,
    line_start: u64,
    next_line_start: Option<u64>,
    horizontal_offset: usize,
) -> io::Result<String> {
    let preview_limit = horizontal_offset.saturating_add(MAX_LINE_PREVIEW_BYTES);

    let mut bytes = if let Some(next_start) = next_line_start {
        let line_len = next_start.saturating_sub(line_start) as usize;
        source.read_at(line_start, line_len.min(preview_limit.saturating_add(2)))?
    } else {
        read_until_newline_or_limit(source, line_start, preview_limit.saturating_add(2))?
    };

    trim_newline_bytes(&mut bytes);

    let skipped = horizontal_offset.min(bytes.len());
    let end = bytes
        .len()
        .min(skipped.saturating_add(MAX_LINE_PREVIEW_BYTES));
    let mut text = decode_lossy(&bytes[skipped..end]).into_owned();

    if skipped > 0 {
        text.insert_str(0, "...");
    }
    if end < bytes.len() || (next_line_start.is_none() && bytes.len() >= preview_limit) {
        text.push_str("...");
    }

    Ok(text)
}

fn read_until_newline_or_limit(
    source: &FileSource,
    start_offset: u64,
    byte_limit: usize,
) -> io::Result<Vec<u8>> {
    let mut offset = start_offset;
    let mut collected = Vec::new();
    let hard_limit = byte_limit.max(1);

    while collected.len() < hard_limit {
        let remaining = hard_limit - collected.len();
        let chunk = source.read_at(offset, remaining.min(READ_CHUNK_SIZE))?;
        if chunk.is_empty() {
            break;
        }

        if let Some(newline_index) = chunk.iter().position(|byte| *byte == b'\n') {
            collected.extend_from_slice(&chunk[..=newline_index]);
            break;
        }

        offset += chunk.len() as u64;
        collected.extend_from_slice(&chunk);
    }

    Ok(collected)
}

fn trim_newline_bytes(bytes: &mut Vec<u8>) {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
}

fn decode_lossy(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

#[cfg(test)]
mod tests {
    use super::render_text_lines;
    use crate::viewer::{file_source::FileSource, line_index::LineIndex};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rcommander_{name}_{unique}.tmp"))
    }

    #[test]
    fn renders_invalid_utf8_lossy() {
        let path = temp_file_path("text_lossy");
        fs::write(&path, [0x66, 0x6F, 0x80, 0x6F, b'\n']).unwrap();

        let source = FileSource::open(&path).unwrap();
        let mut index = LineIndex::new();
        let render = render_text_lines(&source, &mut index, 0, 1, 0).unwrap();

        assert_eq!(render.lines, vec!["fo�o".to_string()]);

        fs::remove_file(path).unwrap();
    }
}
