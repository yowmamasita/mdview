//! The command line: parsing, `--print-html`, and handing off to the window.
//!
//! This lives in the library rather than in `main.rs` because Windows needs two
//! executables built from it — see `src/bin/mdview-console.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::document::{Assets, Document, Theme};
use crate::markdown::RenderOptions;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    /// Document to open.
    path: Option<PathBuf>,
    /// Colour scheme.
    theme: Theme,
    /// Reload when the file changes.
    watch: bool,
    /// Render Mermaid diagrams.
    mermaid: bool,
    /// Print the rendered page to stdout instead of opening a window.
    print_html: bool,
}

const USAGE: &str = "\
mdview — a lightweight Markdown viewer

USAGE:
    mdview [OPTIONS] [FILE]

ARGS:
    <FILE>    Markdown document to open

OPTIONS:
    -t, --theme <auto|light|dark>  Colour scheme (default: auto)
        --no-watch                 Do not reload when the file changes on disk
        --no-mermaid               Do not render Mermaid diagrams
        --print-html               Write a self-contained HTML page to stdout and exit
    -h, --help                     Print this help
    -V, --version                  Print version information

KEYS:
    Ctrl/Cmd+O    open a file        Ctrl/Cmd+R  reload
    Ctrl/Cmd+D    toggle dark mode   Ctrl/Cmd+P  print
    Ctrl/Cmd+/-/0 zoom               g / G       top / bottom
";

/// Parse the command line and do what it asks.
pub fn run() -> ExitCode {
    match parse_args() {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(args)) => match dispatch(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("mdview: {err}");
                ExitCode::FAILURE
            }
        },
        Err(err) => {
            eprintln!("mdview: {err}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Run whichever mode the arguments selected.
fn dispatch(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.print_html {
        return print_html(&args);
    }
    open_window(args)
}

/// Render to stdout — useful for scripting, and for checking output without a
/// display attached.
fn print_html(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = args.path.as_deref() else {
        return Err("--print-html needs a file".into());
    };
    let source = std::fs::read_to_string(path)?;

    let mut opts = RenderOptions::viewer();
    opts.mermaid = args.mermaid;

    let document = Document::from_markdown(&source, &opts, args.theme, &path.to_string_lossy());
    print!("{}", document.to_html(Assets::Inline));
    Ok(())
}

/// Open the viewer window.
#[cfg(feature = "gui")]
fn open_window(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    crate::app::run(crate::app::Options {
        path: args.path,
        theme: args.theme,
        watch: args.watch,
        mermaid: args.mermaid,
    })
}

/// Without the `gui` feature there is no window to open.
#[cfg(not(feature = "gui"))]
fn open_window(_args: Args) -> Result<(), Box<dyn std::error::Error>> {
    Err("this build has no window support; use --print-html".into())
}

/// Parse the command line. `Ok(None)` means the program already did its job
/// (printed help or a version) and should exit.
fn parse_args() -> Result<Option<Args>, Box<dyn std::error::Error>> {
    let mut parser = pico_args::Arguments::from_env();

    if parser.contains(["-h", "--help"]) {
        print!("{USAGE}");
        return Ok(None);
    }
    if parser.contains(["-V", "--version"]) {
        println!("mdview {}", env!("CARGO_PKG_VERSION"));
        println!("mermaid {}", crate::assets::MERMAID_VERSION);
        return Ok(None);
    }

    let theme: Theme = parser
        .opt_value_from_str(["-t", "--theme"])?
        .unwrap_or_default();
    let watch = !parser.contains("--no-watch");
    let mermaid = !parser.contains("--no-mermaid");
    let print_html = parser.contains("--print-html");

    // Anything left over is either the document to open or a mistake. Checking
    // it here — rather than letting the first stray token be taken as a file
    // name — is what makes `mdview --nonsense` say so.
    let mut path: Option<PathBuf> = None;
    for argument in parser.finish() {
        let text = argument.to_string_lossy().into_owned();
        if text.starts_with('-') && text != "-" {
            return Err(format!("unknown option `{text}`").into());
        }
        if path.is_some() {
            return Err(format!("unexpected argument `{text}`").into());
        }
        path = Some(PathBuf::from(argument));
    }

    if let Some(path) = path.as_deref()
        && !path.exists()
    {
        return Err(format!("{}: no such file", path.display()).into());
    }

    Ok(Some(Args {
        path: path.map(canonicalize),
        theme,
        watch,
        mermaid,
        print_html,
    }))
}

/// Absolute paths keep the protocol's URL mapping unambiguous.
fn canonicalize(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}
