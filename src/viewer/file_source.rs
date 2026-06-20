use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::fs::FileExt;
#[cfg(target_family = "windows")]
use std::os::windows::fs::FileExt;

#[derive(Debug)]
pub struct FileSource {
    path: PathBuf,
    file: File,
    len: u64,
}

impl FileSource {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let len = file.metadata()?.len();
        Ok(Self { path, file, len })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn read_at(&self, offset: u64, max_len: usize) -> io::Result<Vec<u8>> {
        if max_len == 0 || offset >= self.len {
            return Ok(Vec::new());
        }

        let available = (self.len - offset) as usize;
        let to_read = available.min(max_len);
        let mut buffer = vec![0; to_read];
        let bytes_read = read_exact_at_most(&self.file, &mut buffer, offset)?;
        buffer.truncate(bytes_read);
        Ok(buffer)
    }
}

fn read_exact_at_most(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(target_family = "unix")]
    {
        file.read_at(buffer, offset)
    }

    #[cfg(target_family = "windows")]
    {
        file.seek_read(buffer, offset)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/viewer_file_source_tests.rs"]
mod tests;
