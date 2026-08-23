//! `mdview` — a lightweight Markdown viewer.
//!
//! On Windows this is the GUI-subsystem executable: the one a file association
//! points at, so double-clicking a `.md` file does not also open a console
//! window. Shells do not wait for a GUI-subsystem process, which makes it a poor
//! fit for redirecting `--print-html`; `mdview-console.exe` exists for that.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

/// Reattach to the console that launched us, if we have no output of our own.
///
/// A `windows` subsystem binary is given no standard handles when it is started
/// from Explorer, so `--help` and `--version` would write into nothing. This
/// gets them back when the caller was a terminal.
///
/// The guard matters: `AttachConsole` *replaces* the standard handles rather
/// than adding them, and a shell has already passed its own down.
#[cfg(windows)]
fn attach_parent_console() {
    /// `ATTACH_PARENT_PROCESS`
    const PARENT: i32 = -1;
    /// `STD_OUTPUT_HANDLE`
    const STD_OUTPUT: u32 = -11i32 as u32;
    /// `INVALID_HANDLE_VALUE`
    const INVALID: isize = -1;

    // Provided by kernel32, which the MSVC target links by default.
    unsafe extern "system" {
        fn GetStdHandle(std_handle: u32) -> isize;
        fn AttachConsole(process_id: i32) -> i32;
    }

    unsafe {
        let stdout = GetStdHandle(STD_OUTPUT);
        if stdout == 0 || stdout == INVALID {
            AttachConsole(PARENT);
        }
    }
}

/// Nothing to do anywhere else.
#[cfg(not(windows))]
fn attach_parent_console() {}

fn main() -> ExitCode {
    attach_parent_console();
    mdview::cli::run()
}
