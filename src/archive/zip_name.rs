//! Decode ZIP entry names that omit the UTF-8 flag.
//!
//! The `zip` crate maps non-UTF-8 names with CP437, which turns Russian OEM
//! (CP866) / Windows-1251 names into mojibake. Prefer Unicode Path (0x7075),
//! then valid UTF-8, then a CP866/CP1251 heuristic based on typical byte ranges.

use encoding_rs::{IBM866, WINDOWS_1251};

const INFOZIP_UNICODE_PATH: u16 = 0x7075;

/// Decode a ZIP central/local header file name to Unicode.
pub fn decode_zip_entry_name(name_raw: &[u8], extra_data: &[u8], crate_decoded: &str) -> String {
    if let Some(unicode) = unicode_path_from_extra(extra_data) {
        return unicode;
    }

    if name_raw.is_empty() {
        return String::new();
    }

    if let Ok(utf8) = std::str::from_utf8(name_raw) {
        return utf8.to_string();
    }

    if let Some(decoded) = best_legacy_decode(name_raw) {
        return decoded;
    }

    crate_decoded.to_string()
}

fn unicode_path_from_extra(extra: &[u8]) -> Option<String> {
    let mut offset = 0;
    while offset + 4 <= extra.len() {
        let header_id = u16::from_le_bytes([extra[offset], extra[offset + 1]]);
        let data_size = u16::from_le_bytes([extra[offset + 2], extra[offset + 3]]) as usize;
        offset += 4;
        if offset + data_size > extra.len() {
            break;
        }
        let data = &extra[offset..offset + data_size];
        offset += data_size;

        if header_id != INFOZIP_UNICODE_PATH || data.len() < 5 || data[0] != 1 {
            continue;
        }
        // Version 1: CRC32 of the File Name field (skipped) + UTF-8 name.
        if let Ok(name) = std::str::from_utf8(&data[5..]) {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn best_legacy_decode(raw: &[u8]) -> Option<String> {
    let score_866 = oem866_byte_score(raw);
    let score_1251 = windows1251_byte_score(raw);
    if score_866 == 0 && score_1251 == 0 {
        return None;
    }

    // Prefer OEM CP866 on ties — typical for Russian ZIPs created on Windows.
    let encoding = if score_866 >= score_1251 {
        IBM866
    } else {
        WINDOWS_1251
    };

    let (decoded, _, had_errors) = encoding.decode(raw);
    if had_errors {
        return None;
    }
    let text = decoded.into_owned();
    contains_cyrillic(&text).then_some(text)
}

/// CP866 Cyrillic letters live mainly in 0x80–0xAF and 0xE0–0xF1.
fn oem866_byte_score(raw: &[u8]) -> usize {
    raw.iter()
        .filter(|&&b| (0x80..=0xAF).contains(&b) || (0xE0..=0xF1).contains(&b))
        .count()
}

/// Windows-1251 Cyrillic letters live mainly in 0xC0–0xFF.
fn windows1251_byte_score(raw: &[u8]) -> usize {
    raw.iter().filter(|&&b| (0xC0..=0xFF).contains(&b)).count()
}

fn contains_cyrillic(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::Encoding;

    fn encode(encoding: &'static Encoding, text: &str) -> Vec<u8> {
        let (bytes, _, _) = encoding.encode(text);
        bytes.into_owned()
    }

    #[test]
    fn keeps_ascii_and_utf8() {
        assert_eq!(
            decode_zip_entry_name(b"readme.txt", b"", "readme.txt"),
            "readme.txt"
        );
        let utf8 = "Документ.txt".as_bytes();
        assert_eq!(decode_zip_entry_name(utf8, b"", "mojibake"), "Документ.txt");
    }

    #[test]
    fn decodes_cp866_russian_name() {
        let raw = encode(IBM866, "Привет.txt");
        assert_eq!(decode_zip_entry_name(&raw, b"", "garbage"), "Привет.txt");
    }

    #[test]
    fn decodes_windows_1251_russian_name() {
        let raw = encode(WINDOWS_1251, "Папка/файл.doc");
        assert_eq!(
            decode_zip_entry_name(&raw, b"", "mojibake"),
            "Папка/файл.doc"
        );
    }

    #[test]
    fn prefers_infozip_unicode_path_extra() {
        let raw = encode(IBM866, "wrong.txt");
        let unicode = "Правильно.txt";
        let mut extra = Vec::new();
        extra.extend_from_slice(&INFOZIP_UNICODE_PATH.to_le_bytes());
        let mut payload = vec![1, 0, 0, 0, 0]; // version + dummy CRC
        payload.extend_from_slice(unicode.as_bytes());
        extra.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        extra.extend_from_slice(&payload);

        assert_eq!(
            decode_zip_entry_name(&raw, &extra, "mojibake"),
            "Правильно.txt"
        );
    }

    #[test]
    fn falls_back_when_no_cyrillic_signal() {
        // CP866 box-drawing bytes — not Cyrillic letter ranges for scoring.
        let raw = [0xB0, 0xB1, 0xB2, b'.', b't', b'x', b't'];
        assert_eq!(
            decode_zip_entry_name(&raw, b"", "fallback.txt"),
            "fallback.txt"
        );
    }
}
