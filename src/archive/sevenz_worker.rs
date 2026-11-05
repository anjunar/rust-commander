//! Out-of-process 7z helper for Windows.
//!
//! Opening `.7z` inside the GTK UI process can load the installed 7-Zip shell
//! extension and abort with STATUS_STACK_BUFFER_OVERRUN. Listing and extraction
//! therefore run in a short-lived child (`rcommander --archive-worker ...`).

use std::{
    io::{self, Write},
    path::Path,
    time::UNIX_EPOCH,
};

#[cfg(not(test))]
use std::{
    ffi::OsString,
    process::{Command, Output},
    time::Duration,
};

use super::{sevenz_backend::SevenZipBackend, ArchiveEntry, ArchiveEntryKind};

#[cfg(not(test))]
use super::ArchiveError;

pub const WORKER_FLAG: &str = "--archive-worker";

/// Returns `Some(exit_code)` when this process is the archive worker.
pub fn maybe_run_from_args<I, S>(args: I) -> Option<i32>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut args = args.into_iter();
    let _exe = args.next()?;
    let flag = args.next()?;
    if flag.as_ref() != std::ffi::OsStr::new(WORKER_FLAG) {
        return None;
    }

    let command = args
        .next()
        .and_then(|value| value.as_ref().to_str().map(str::to_owned));
    let exit = match command.as_deref() {
        Some("list") => match args.next() {
            Some(path) => run_list(Path::new(&path)),
            None => fail_usage("missing archive path"),
        },
        Some("extract") => {
            let archive = args.next();
            let target = args.next();
            match (archive, target) {
                (Some(archive), Some(target)) => {
                    let entries = args
                        .map(|value| value.as_ref().to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    run_extract(Path::new(&archive), Path::new(&target), &entries)
                }
                _ => fail_usage("usage: extract <archive> <target> [entries...]"),
            }
        }
        Some(other) => fail_usage(&format!("unknown archive-worker command: {other}")),
        None => fail_usage("missing archive-worker command"),
    };
    Some(exit)
}

fn fail_usage(message: &str) -> i32 {
    let _ = writeln!(io::stderr(), "{message}");
    2
}

fn run_list(path: &Path) -> i32 {
    match SevenZipBackend::new().open_in_process(path) {
        Ok(session) => match write_list_stdout(session.cached_entries()) {
            Ok(()) => 0,
            Err(error) => fail_worker(&error.to_string()),
        },
        Err(error) => fail_worker(&error.to_string()),
    }
}

fn run_extract(archive: &Path, target: &Path, entries: &[String]) -> i32 {
    let backend = SevenZipBackend::new();
    let session = match backend.open_in_process(archive) {
        Ok(session) => session,
        Err(error) => return fail_worker(&error.to_string()),
    };

    let owned;
    let entry_paths = if entries.is_empty() {
        owned = session
            .cached_entries()
            .iter()
            .map(|entry| entry.archive_path.clone())
            .collect::<Vec<_>>();
        owned.as_slice()
    } else {
        entries
    };

    match backend.extract_entries_in_process(&session, entry_paths, target) {
        Ok(()) => {
            let _ = writeln!(io::stdout(), "OK");
            0
        }
        Err(error) => fail_worker(&error.to_string()),
    }
}

fn fail_worker(detail: &str) -> i32 {
    let _ = writeln!(io::stderr(), "{detail}");
    1
}

fn write_list_stdout(entries: &[ArchiveEntry]) -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "OK")?;
    for entry in entries {
        let kind = match entry.kind {
            ArchiveEntryKind::Directory => 'D',
            ArchiveEntryKind::File => 'F',
        };
        let mtime = entry
            .modified_time
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|| "-".into());
        let attrs = entry.attributes.as_deref().unwrap_or("-");
        // path may contain spaces; use a record separator unlikely in archive paths.
        writeln!(
            out,
            "{kind}\x1f{}\x1f{}\x1f{mtime}\x1f{attrs}",
            entry.archive_path.replace('\x1f', ""),
            entry.size
        )?;
    }
    Ok(())
}

/// Parent-process helpers (not used when `cfg(test)` forces in-process 7z).
#[cfg(not(test))]
pub fn list_via_helper(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let output = run_worker(&[OsString::from("list"), path.as_os_str().to_os_string()])?;
    if !output.status.success() {
        return Err(list_failed(path, &output));
    }
    parse_list_stdout(&output.stdout).map_err(|detail| ArchiveError::ListFailed {
        path: path.to_path_buf(),
        detail,
    })
}

#[cfg(not(test))]
pub fn extract_via_helper(
    archive: &Path,
    target_dir: &Path,
    entry_paths: &[String],
) -> Result<(), ArchiveError> {
    let mut args = vec![
        OsString::from("extract"),
        archive.as_os_str().to_os_string(),
        target_dir.as_os_str().to_os_string(),
    ];
    args.extend(entry_paths.iter().map(OsString::from));
    let output = run_worker(&args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ArchiveError::ExtractionFailed {
            path: archive.to_path_buf(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

#[cfg(not(test))]
fn run_worker(args: &[OsString]) -> Result<Output, ArchiveError> {
    let exe = std::env::current_exe().map_err(|error| ArchiveError::IoError {
        detail: format!("Could not resolve application executable: {error}"),
    })?;

    let mut command = Command::new(&exe);
    command.arg(WORKER_FLAG);
    for arg in args {
        command.arg(arg);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().map_err(|error| ArchiveError::IoError {
        detail: format!("Could not start 7z helper process: {error}"),
    })?;

    if output
        .status
        .code()
        .is_some_and(|code| code as u32 == 0xC000_0409)
        || matches!(output.status.code(), Some(code) if code < 0)
    {
        let archive = args
            .get(1)
            .map(|value| Path::new(value).display().to_string())
            .unwrap_or_else(|| "<archive>".into());
        return Err(ArchiveError::LibraryError {
            library: "sevenz-worker".into(),
            detail: format!(
                "7z helper aborted while opening {archive} (Windows shell association crash)"
            ),
        });
    }

    Ok(output)
}

#[cfg(not(test))]
fn list_failed(path: &Path, output: &Output) -> ArchiveError {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    ArchiveError::ListFailed {
        path: path.to_path_buf(),
        detail: if detail.is_empty() {
            format!("helper exit {:?}", output.status.code())
        } else {
            detail
        },
    }
}

#[cfg(not(test))]
fn parse_list_stdout(bytes: &[u8]) -> Result<Vec<ArchiveEntry>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut lines = text.lines();
    match lines.next() {
        Some("OK") => {}
        Some(other) => return Err(format!("unexpected helper header: {other}")),
        None => return Err("empty helper output".into()),
    }

    let mut entries = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\x1f');
        let kind = parts.next().ok_or_else(|| "missing kind".to_string())?;
        let archive_path = parts
            .next()
            .ok_or_else(|| "missing path".to_string())?
            .to_string();
        let size = parts
            .next()
            .ok_or_else(|| "missing size".to_string())?
            .parse::<u64>()
            .map_err(|error| error.to_string())?;
        let mtime = parts.next().unwrap_or("-");
        let attrs = parts.next().unwrap_or("-");

        let kind = match kind {
            "D" => ArchiveEntryKind::Directory,
            "F" => ArchiveEntryKind::File,
            other => return Err(format!("unknown entry kind: {other}")),
        };
        let modified_time = if mtime == "-" {
            None
        } else {
            mtime
                .parse::<u64>()
                .ok()
                .map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
        };
        let attributes = if attrs == "-" {
            None
        } else {
            Some(attrs.to_string())
        };

        entries.push(ArchiveEntry {
            display_name: super::archive_entry_display_name(&archive_path),
            archive_path,
            kind,
            size,
            modified_time,
            attributes,
        });
    }

    Ok(entries)
}
