//! `mdview-console` — the console-subsystem build of `mdview`, for Windows.
//!
//! `mdview.exe` is a GUI-subsystem executable so that opening a document from
//! Explorer does not raise a console window alongside the viewer. The cost is
//! that neither `cmd` nor PowerShell waits for such a process: they close the
//! pipe as soon as the command returns, and anything still being written to it
//! fails. That makes `mdview.exe --print-html doc.md > doc.html` unreliable.
//!
//! This binary is identical apart from its subsystem, so a shell waits for it
//! and redirection behaves. It is the one to use in scripts.

use std::process::ExitCode;

fn main() -> ExitCode {
    mdview::cli::run()
}
