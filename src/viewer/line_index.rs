use std::io;

use memchr::memchr_iter;

use crate::viewer::file_source::FileSource;

const DEFAULT_SCAN_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<u64>,
    indexed_until: u64,
    is_complete: bool,
    scan_chunk_size: usize,
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LineIndex {
    pub fn new() -> Self {
        Self {
            line_starts: vec![0],
            indexed_until: 0,
            is_complete: false,
            scan_chunk_size: DEFAULT_SCAN_CHUNK_SIZE,
        }
    }

    pub fn line_start(&self, line: usize) -> Option<u64> {
        self.line_starts.get(line).copied()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub fn indexed_until(&self) -> u64 {
        self.indexed_until
    }

    pub fn build_initial(
        source: &FileSource,
        initial_bytes: usize,
        scan_chunk_size: usize,
    ) -> io::Result<Self> {
        let mut index = Self {
            scan_chunk_size: scan_chunk_size.max(1),
            ..Self::new()
        };
        index.scan_until_offset(source, initial_bytes as u64)?;
        Ok(index)
    }

    pub fn ensure_lines(&mut self, source: &FileSource, target_line: usize) -> io::Result<()> {
        while !self.is_complete && self.line_starts.len() <= target_line {
            self.scan_next_chunk(source)?;
        }
        Ok(())
    }

    pub fn ensure_complete(&mut self, source: &FileSource) -> io::Result<()> {
        while !self.is_complete {
            self.scan_next_chunk(source)?;
        }
        Ok(())
    }

    pub fn scan_until_offset(&mut self, source: &FileSource, target_offset: u64) -> io::Result<()> {
        while !self.is_complete && self.indexed_until < target_offset.min(source.len()) {
            self.scan_next_chunk(source)?;
        }
        Ok(())
    }

    fn scan_next_chunk(&mut self, source: &FileSource) -> io::Result<()> {
        if self.is_complete {
            return Ok(());
        }

        let bytes = source.read_at(self.indexed_until, self.scan_chunk_size)?;
        if bytes.is_empty() {
            self.is_complete = true;
            return Ok(());
        }

        for newline_index in memchr_iter(b'\n', &bytes) {
            let next_line_start = self.indexed_until + newline_index as u64 + 1;
            if self.line_starts.last().copied() != Some(next_line_start) {
                self.line_starts.push(next_line_start);
            }
        }

        self.indexed_until = (self.indexed_until + bytes.len() as u64).min(source.len());
        if self.indexed_until >= source.len() {
            self.is_complete = true;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LineIndex;
    use crate::viewer::file_source::FileSource;
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
    fn indexes_multiple_lines() {
        let path = temp_file_path("line_index_multi");
        fs::write(&path, b"one\ntwo\nthree").unwrap();

        let source = FileSource::open(&path).unwrap();
        let mut index = LineIndex::new();
        index.ensure_complete(&source).unwrap();

        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_start(0), Some(0));
        assert_eq!(index.line_start(1), Some(4));
        assert_eq!(index.line_start(2), Some(8));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn indexes_empty_file() {
        let path = temp_file_path("line_index_empty");
        fs::write(&path, b"").unwrap();

        let source = FileSource::open(&path).unwrap();
        let mut index = LineIndex::new();
        index.ensure_complete(&source).unwrap();

        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_start(0), Some(0));
        assert!(index.is_complete());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn tracks_indexed_offset() {
        let path = temp_file_path("line_index_progress");
        fs::write(&path, b"one\ntwo\nthree\nfour\n").unwrap();

        let source = FileSource::open(&path).unwrap();
        let index = LineIndex::build_initial(&source, 5, 5).unwrap();

        assert!(index.indexed_until() >= 5);

        fs::remove_file(path).unwrap();
    }
}
