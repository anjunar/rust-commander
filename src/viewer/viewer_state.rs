use std::{io, path::Path};

use crate::viewer::{
    file_source::FileSource,
    hex_view::{render_hex_lines, total_hex_lines},
    line_index::LineIndex,
    text_view::render_text_lines,
};

const BINARY_DETECTION_BYTES: usize = 4096;
const INITIAL_INDEX_BYTES: usize = 4 * 1024 * 1024;
const INDEX_SCAN_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerMode {
    Text,
    Hex,
}

#[derive(Debug, Clone)]
pub struct RenderedContent {
    pub title: String,
    pub status: String,
    pub body: String,
}

#[derive(Debug)]
pub struct ViewerState {
    source: FileSource,
    line_index: LineIndex,
    first_visible_line: usize,
    horizontal_offset: usize,
    mode: ViewerMode,
    visible_lines: usize,
}

impl ViewerState {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let source = FileSource::open(path)?;
        let detection_bytes = source.read_at(0, BINARY_DETECTION_BYTES)?;
        let mode = if detection_bytes.contains(&0) {
            ViewerMode::Hex
        } else {
            ViewerMode::Text
        };

        let line_index = if mode == ViewerMode::Text {
            LineIndex::build_initial(&source, INITIAL_INDEX_BYTES, INDEX_SCAN_CHUNK_BYTES)?
        } else {
            LineIndex::new()
        };

        Ok(Self {
            source,
            line_index,
            first_visible_line: 0,
            horizontal_offset: 0,
            mode,
            visible_lines: 40,
        })
    }

    pub fn path(&self) -> &Path {
        self.source.path()
    }

    pub fn file_len(&self) -> u64 {
        self.source.len()
    }

    pub fn mode(&self) -> ViewerMode {
        self.mode
    }

    pub fn first_visible_line(&self) -> usize {
        self.first_visible_line
    }

    pub fn set_first_visible_line(&mut self, line: usize) -> io::Result<()> {
        self.first_visible_line = line;
        self.clamp_after_indexing()?;
        Ok(())
    }

    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }

    pub fn visible_lines(&self) -> usize {
        self.visible_lines
    }

    pub fn set_visible_lines(&mut self, visible_lines: usize) {
        self.visible_lines = visible_lines.max(1);
    }

    pub fn scroll_line_up(&mut self) {
        self.first_visible_line = self.first_visible_line.saturating_sub(1);
    }

    pub fn scroll_line_down(&mut self) {
        if self.mode == ViewerMode::Text && !self.line_index.is_complete() {
            self.first_visible_line = self.first_visible_line.saturating_add(1);
            return;
        }

        let max_first = self.max_first_visible_line();
        self.first_visible_line = self.first_visible_line.saturating_add(1).min(max_first);
    }

    pub fn page_up(&mut self) {
        self.first_visible_line = self.first_visible_line.saturating_sub(self.visible_lines);
    }

    pub fn page_down(&mut self) {
        if self.mode == ViewerMode::Text && !self.line_index.is_complete() {
            self.first_visible_line = self.first_visible_line.saturating_add(self.visible_lines);
            return;
        }

        let max_first = self.max_first_visible_line();
        self.first_visible_line = self
            .first_visible_line
            .saturating_add(self.visible_lines)
            .min(max_first);
    }

    pub fn go_to_start(&mut self) {
        self.first_visible_line = 0;
    }

    pub fn go_to_end(&mut self) -> io::Result<()> {
        self.first_visible_line = self.max_first_visible_line_for_current_mode()?;
        Ok(())
    }

    pub fn scroll_left(&mut self) {
        self.horizontal_offset = self.horizontal_offset.saturating_sub(8);
    }

    pub fn scroll_right(&mut self) {
        self.horizontal_offset = self.horizontal_offset.saturating_add(8);
    }

    pub fn toggle_hex_mode(&mut self) {
        self.mode = match self.mode {
            ViewerMode::Text => ViewerMode::Hex,
            ViewerMode::Hex => ViewerMode::Text,
        };
        self.first_visible_line = 0;
        self.horizontal_offset = 0;
    }

    pub fn render(&mut self) -> io::Result<RenderedContent> {
        self.clamp_after_indexing()?;

        let title = format!("View {}", file_label(self.path()));
        let body = match self.mode {
            ViewerMode::Text => {
                let render = render_text_lines(
                    &self.source,
                    &mut self.line_index,
                    self.first_visible_line,
                    self.visible_lines,
                    self.horizontal_offset,
                )?;
                if render.lines.is_empty() {
                    String::new()
                } else {
                    render.lines.join("\n")
                }
            }
            ViewerMode::Hex => {
                let lines =
                    render_hex_lines(&self.source, self.first_visible_line, self.visible_lines)?;
                lines.join("\n")
            }
        };

        Ok(RenderedContent {
            title,
            status: self.status_line(),
            body,
        })
    }

    pub fn status_line(&self) -> String {
        let mode = match self.mode {
            ViewerMode::Text => "Text",
            ViewerMode::Hex => "Hex",
        };
        format!(
            "{mode} | {} | line {} | col {} | F2 toggle mode | Esc close",
            format_bytes(self.source.len()),
            self.first_visible_line.saturating_add(1),
            self.horizontal_offset.saturating_add(1),
        )
    }

    fn max_first_visible_line(&self) -> usize {
        match self.mode {
            ViewerMode::Hex => self
                .max_first_for_line_count(total_hex_lines(self.source.len()))
                .unwrap_or(0),
            ViewerMode::Text => self
                .line_index
                .line_count()
                .saturating_sub(self.visible_lines),
        }
    }

    fn max_first_visible_line_for_current_mode(&mut self) -> io::Result<usize> {
        match self.mode {
            ViewerMode::Hex => Ok(self
                .max_first_for_line_count(total_hex_lines(self.source.len()))
                .unwrap_or(0)),
            ViewerMode::Text => {
                self.line_index.ensure_complete(&self.source)?;
                Ok(self
                    .max_first_for_line_count(self.line_index.line_count())
                    .unwrap_or(0))
            }
        }
    }

    fn max_first_for_line_count(&self, line_count: usize) -> Option<usize> {
        Some(line_count.saturating_sub(self.visible_lines))
    }

    pub fn estimated_total_lines(&self) -> usize {
        match self.mode {
            ViewerMode::Hex => total_hex_lines(self.source.len()),
            ViewerMode::Text => self.estimated_text_total_lines(),
        }
    }

    pub fn has_complete_line_count(&self) -> bool {
        match self.mode {
            ViewerMode::Hex => true,
            ViewerMode::Text => self.line_index.is_complete(),
        }
    }

    fn clamp_after_indexing(&mut self) -> io::Result<()> {
        if self.mode != ViewerMode::Text {
            return Ok(());
        }

        self.line_index
            .ensure_lines(&self.source, self.first_visible_line)?;
        if self
            .line_index
            .line_start(self.first_visible_line)
            .is_none()
            && self.line_index.is_complete()
        {
            self.first_visible_line = self
                .max_first_for_line_count(self.line_index.line_count())
                .unwrap_or(0);
        }

        Ok(())
    }

    fn estimated_text_total_lines(&self) -> usize {
        if self.line_index.is_complete() {
            return self.line_index.line_count().max(1);
        }

        let indexed_until = self.line_index.indexed_until();
        if indexed_until == 0 {
            return self
                .line_index
                .line_count()
                .max(self.first_visible_line + self.visible_lines)
                .max(1);
        }

        let indexed_lines = self.line_index.line_count().max(1) as f64;
        let average_bytes_per_line = (indexed_until as f64 / indexed_lines).max(1.0);
        let estimated = (self.source.len() as f64 / average_bytes_per_line).ceil() as usize;

        estimated
            .max(self.line_index.line_count())
            .max(self.first_visible_line + self.visible_lines)
            .max(1)
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::{ViewerMode, ViewerState};
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
    fn scrolling_does_not_underflow() {
        let path = temp_file_path("viewer_scroll");
        fs::write(&path, b"one\ntwo\n").unwrap();

        let mut state = ViewerState::open(&path).unwrap();
        state.scroll_line_up();
        assert_eq!(state.first_visible_line(), 0);

        state.page_up();
        assert_eq!(state.first_visible_line(), 0);

        state.scroll_line_down();
        assert_eq!(state.first_visible_line(), 0);

        state.toggle_hex_mode();
        assert_eq!(state.mode(), ViewerMode::Hex);
        state.scroll_line_up();
        assert_eq!(state.first_visible_line(), 0);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn estimates_more_than_indexed_lines_for_large_text_files() {
        let path = temp_file_path("viewer_estimate");
        let content = "line\n".repeat(2_000);
        fs::write(&path, content).unwrap();

        let state = ViewerState::open(&path).unwrap();

        assert!(state.estimated_total_lines() > 100);

        fs::remove_file(path).unwrap();
    }
}
