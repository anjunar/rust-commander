use std::io;

use crate::viewer::file_source::FileSource;

const BYTES_PER_LINE: usize = 16;

pub fn render_hex_lines(
    source: &FileSource,
    first_line: usize,
    visible_lines: usize,
) -> io::Result<Vec<String>> {
    if visible_lines == 0 {
        return Ok(Vec::new());
    }

    let start_offset = first_line as u64 * BYTES_PER_LINE as u64;
    let max_len = visible_lines.saturating_mul(BYTES_PER_LINE);
    let bytes = source.read_at(start_offset, max_len)?;

    if bytes.is_empty() && source.is_empty() && first_line == 0 {
        return Ok(vec!["00000000".to_string()]);
    }

    Ok(render_hex_lines_from_bytes(start_offset, &bytes))
}

pub fn render_hex_lines_from_bytes(start_offset: u64, bytes: &[u8]) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }

    bytes
        .chunks(BYTES_PER_LINE)
        .enumerate()
        .map(|(index, chunk)| {
            render_hex_line(start_offset + (index * BYTES_PER_LINE) as u64, chunk)
        })
        .collect()
}

pub fn render_hex_line(offset: u64, bytes: &[u8]) -> String {
    let mut hex_columns = Vec::with_capacity(BYTES_PER_LINE);
    for byte_index in 0..BYTES_PER_LINE {
        if let Some(byte) = bytes.get(byte_index) {
            hex_columns.push(format!("{byte:02X}"));
        } else {
            hex_columns.push("  ".to_string());
        }
    }

    let left = hex_columns[..8].join(" ");
    let right = hex_columns[8..].join(" ");
    let ascii = bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect::<String>();

    format!("{offset:08X}  {left}  {right}  |{ascii:<16}|")
}

pub fn total_hex_lines(file_len: u64) -> usize {
    if file_len == 0 {
        1
    } else {
        file_len.div_ceil(BYTES_PER_LINE as u64) as usize
    }
}

#[cfg(test)]
#[path = "../../tests/unit/viewer_hex_view_tests.rs"]
mod tests;
