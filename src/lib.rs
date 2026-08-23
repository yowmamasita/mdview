//! `mdview` — a lightweight Markdown viewer.
//!
//! The crate is split so that everything except the window itself is testable
//! without a display:
//!
//! * [`markdown`] turns Markdown into an HTML fragment (CommonMark + GFM).
//! * [`autolink`] implements GFM's extended autolinks.
//! * [`sanitize`] enforces the HTML allowlist.
//! * [`document`] wraps a fragment in the standalone page the webview loads.
//! * [`assets`] holds the vendored stylesheet, viewer script and Mermaid runtime.
//! * [`protocol`] maps webview requests onto the filesystem.
//! * [`cli`] is the command line, shared by both Windows executables.

pub mod assets;
pub mod autolink;
pub mod cli;
pub mod document;
pub mod markdown;
pub mod protocol;
pub mod sanitize;

#[cfg(feature = "gui")]
pub mod app;
#[cfg(feature = "gui")]
pub mod watch;

pub use document::{Document, Theme};
pub use markdown::{Flavor, Heading, RenderOptions, Rendered, render};

/// File extensions treated as Markdown.
pub const MARKDOWN_EXTENSIONS: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext", "rmd", "qmd",
];

/// Whether `path` looks like a Markdown document.
pub fn is_markdown_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| MARKDOWN_EXTENSIONS.contains(&e.as_str()))
}
