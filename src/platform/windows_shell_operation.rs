//! File operations through the Windows shell, the way Explorer runs them.
//!
//! With elevation the shell asks for administrator rights and does the work in
//! its own elevated component, so no code of this application runs with them.
//! The shell also brings its own progress and conflict dialogs.

use std::{
    ffi::{c_void, OsString},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use windows::{
    core::{HRESULT, HSTRING, PCWSTR},
    Win32::{
        Foundation::{ERROR_CANCELLED, HWND},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            COINIT_DISABLE_OLE1DDE,
        },
        UI::Shell::{
            FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
            COPYENGINE_E_USER_CANCELLED, FILEOPERATION_FLAGS, FOFX_RECYCLEONDELETE,
            FOFX_REQUIREELEVATION, FOFX_SHOWELEVATIONPROMPT, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
            FOF_NOCONFIRMMKDIR, FOF_WANTNUKEWARNING,
        },
    },
};

/// Where a copy or move goes.
#[derive(Clone, Debug)]
pub struct ShellTarget {
    pub directory: PathBuf,
    /// The new name of a single source; `None` keeps the name.
    pub name: Option<OsString>,
    /// Replace existing files without asking.
    pub overwrite_silently: bool,
}

#[derive(Clone, Debug)]
pub enum ShellAction {
    Copy(ShellTarget),
    Move(ShellTarget),
    Delete { recycle: bool },
}

#[derive(Clone, Debug)]
pub struct ShellOperation {
    pub action: ShellAction,
    pub sources: Vec<PathBuf>,
    /// Go straight to the administrator prompt, whether or not it is needed.
    pub elevate: bool,
    /// The native window the dialogs belong to; 0 for none.
    pub owner: isize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellOutcome {
    Completed,
    /// The user declined the prompt or stopped the operation.
    Aborted,
}

/// Runs `operation` and blocks until it ended. Call it off the UI thread: the
/// shell needs a COM apartment of its own and pumps it while it waits.
pub fn perform(operation: &ShellOperation) -> Result<ShellOutcome> {
    unsafe {
        let initialized =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let outcome = run(operation);
        if initialized {
            CoUninitialize();
        }
        outcome
    }
}

unsafe fn run(operation: &ShellOperation) -> Result<ShellOutcome> {
    let shell: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)
        .context("Windows shell not available")?;
    shell.SetOwnerWindow(HWND(operation.owner as *mut c_void))?;
    shell.SetOperationFlags(flags(operation))?;

    match &operation.action {
        ShellAction::Copy(target) => {
            let (directory, name) = destination(target)?;
            for source in &operation.sources {
                shell.CopyItem(&item(source)?, &directory, name_param(&name), None)?;
            }
        }
        ShellAction::Move(target) => {
            let (directory, name) = destination(target)?;
            for source in &operation.sources {
                shell.MoveItem(&item(source)?, &directory, name_param(&name), None)?;
            }
        }
        ShellAction::Delete { .. } => {
            for source in &operation.sources {
                shell.DeleteItem(&item(source)?, None)?;
            }
        }
    }

    match shell.PerformOperations() {
        Ok(()) if shell.GetAnyOperationsAborted()?.as_bool() => Ok(ShellOutcome::Aborted),
        Ok(()) => Ok(ShellOutcome::Completed),
        Err(error) if is_cancellation(error.code()) => Ok(ShellOutcome::Aborted),
        Err(error) => Err(error.into()),
    }
}

fn flags(operation: &ShellOperation) -> FILEOPERATION_FLAGS {
    let elevation = if operation.elevate {
        // The user chose this already; the shell need not ask whether to go on.
        FOFX_SHOWELEVATIONPROMPT | FOFX_REQUIREELEVATION
    } else {
        FILEOPERATION_FLAGS(0)
    };
    let action = match &operation.action {
        ShellAction::Copy(target) | ShellAction::Move(target) if target.overwrite_silently => {
            FOF_NOCONFIRMATION
        }
        ShellAction::Copy(_) | ShellAction::Move(_) => FILEOPERATION_FLAGS(0),
        // Whatever cannot go to the recycle bin must not vanish without a word.
        ShellAction::Delete { recycle: true } => {
            FOF_NOCONFIRMATION | FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING
        }
        ShellAction::Delete { recycle: false } => FOF_NOCONFIRMATION,
    };
    FOF_NOCONFIRMMKDIR | elevation | action
}

fn is_cancellation(code: HRESULT) -> bool {
    code == ERROR_CANCELLED.to_hresult() || code == COPYENGINE_E_USER_CANCELLED
}

unsafe fn destination(target: &ShellTarget) -> Result<(IShellItem, Option<HSTRING>)> {
    Ok((
        item(&target.directory)?,
        target.name.as_deref().map(HSTRING::from),
    ))
}

fn name_param(name: &Option<HSTRING>) -> PCWSTR {
    name.as_ref()
        .map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr()))
}

unsafe fn item(path: &Path) -> Result<IShellItem> {
    SHCreateItemFromParsingName(&HSTRING::from(path), None)
        .with_context(|| format!("Could not find {}", path.display()))
}

#[cfg(test)]
#[path = "../../tests/unit/platform_windows_shell_operation_tests.rs"]
mod tests;
