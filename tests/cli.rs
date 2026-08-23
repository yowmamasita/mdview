//! End-to-end tests of the `mdview` binary.
//!
//! These cover everything reachable without a display; the window itself is
//! exercised by hand.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the binary under test, provided by Cargo.
const BIN: &str = env!("CARGO_BIN_EXE_mdview");

/// Run the binary and capture its output.
fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A scratch directory that cleans itself up.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mdview-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// -------------------------------------------------------------------- help

#[test]
fn help_is_printed_to_stdout_and_succeeds() {
    for flag in ["-h", "--help"] {
        let output = run(&[flag]);
        assert!(output.status.success(), "{flag} failed");
        let text = stdout(&output);
        assert!(text.contains("USAGE"), "{flag}: {text}");
        assert!(text.contains("--theme"), "{flag}: {text}");
        assert!(text.contains("--print-html"), "{flag}: {text}");
        assert!(stderr(&output).is_empty(), "{flag} wrote to stderr");
    }
}

#[test]
fn version_reports_both_the_app_and_the_mermaid_runtime() {
    for flag in ["-V", "--version"] {
        let output = run(&[flag]);
        assert!(output.status.success(), "{flag} failed");
        let text = stdout(&output);
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
        assert!(text.contains("mermaid"), "{text}");
    }
}

// -------------------------------------------------------------- print-html

#[test]
fn print_html_writes_a_standalone_page() {
    let scratch = Scratch::new("print");
    let path = scratch.write("doc.md", "# Title\n\nSome **text**.\n");

    let output = run(&["--print-html", path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));

    let html = stdout(&output);
    assert!(html.starts_with("<!doctype html>"), "{html:.100}");
    assert!(html.contains("<title>Title</title>"), "{html:.600}");
    assert!(html.contains("<strong>text</strong>"));
    // Standalone means no external references at all.
    assert!(!html.contains("/__mdview__/"), "page is not self-contained");
    assert!(html.contains("<style>"));
}

#[test]
fn print_html_embeds_mermaid_only_when_needed() {
    let scratch = Scratch::new("print-mermaid");
    let plain = scratch.write("plain.md", "# Plain\n");
    let diagram = scratch.write("diagram.md", "```mermaid\ngraph TD\nA-->B\n```\n");

    let small = stdout(&run(&["--print-html", plain.to_str().unwrap()]));
    let large = stdout(&run(&["--print-html", diagram.to_str().unwrap()]));

    assert!(small.len() < 200_000, "plain page is {} bytes", small.len());
    assert!(large.len() > small.len(), "diagram page should be larger");
    assert!(large.contains(r#"<pre class="mermaid">"#));
}

#[test]
fn no_mermaid_leaves_the_fence_as_code() {
    let scratch = Scratch::new("no-mermaid");
    let path = scratch.write("d.md", "```mermaid\ngraph TD\n```\n");

    let html = stdout(&run(&[
        "--print-html",
        "--no-mermaid",
        path.to_str().unwrap(),
    ]));
    assert!(!html.contains(r#"<pre class="mermaid">"#), "{html:.900}");
    assert!(html.contains("language-mermaid"));
}

#[test]
fn the_theme_flag_reaches_the_page() {
    let scratch = Scratch::new("theme");
    let path = scratch.write("d.md", "# T\n");

    for theme in ["auto", "light", "dark"] {
        let html = stdout(&run(&["--print-html", "-t", theme, path.to_str().unwrap()]));
        assert!(
            html.contains(&format!(r#"data-theme-preference="{theme}""#)),
            "{theme}: {html:.300}"
        );
    }
}

#[test]
fn print_html_sanitizes() {
    let scratch = Scratch::new("sanitize");
    let path = scratch.write(
        "d.md",
        "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n",
    );

    let html = stdout(&run(&["--print-html", path.to_str().unwrap()]));
    let body_start = html.find("<body>").expect("a body");
    let body = &html[body_start..html.find("<script>").unwrap_or(html.len())];
    // The script is shown as text, the way GitHub shows it — never executed.
    assert!(!body.contains("<script"), "{body}");
    assert!(body.contains("&lt;script&gt;"), "{body}");
    assert!(!body.contains("onerror"), "{body}");
}

#[test]
fn utf8_content_survives_the_round_trip() {
    let scratch = Scratch::new("utf8");
    let path = scratch.write("d.md", "# Ünïcödé — 日本語 — emoji 🎉\n");

    let html = stdout(&run(&["--print-html", path.to_str().unwrap()]));
    assert!(html.contains("Ünïcödé"), "{html:.600}");
    assert!(html.contains("日本語"));
    assert!(html.contains("🎉"));
}

#[test]
fn an_empty_file_renders_the_welcome_body() {
    let scratch = Scratch::new("empty");
    let path = scratch.write("empty.md", "");

    let output = run(&["--print-html", path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let html = stdout(&output);
    assert!(html.contains("md-empty"), "{html:.900}");
    assert!(html.contains("<title>empty.md</title>"), "{html:.600}");
}

// ------------------------------------------------------------------ errors

#[test]
fn a_missing_file_is_reported_without_a_panic() {
    let output = run(&["/definitely/not/here.md"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("no such file"), "{text}");
    assert!(!text.contains("panicked"), "{text}");
}

#[test]
fn an_unknown_theme_is_rejected() {
    let output = run(&["--theme", "sepia", "--print-html"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("unknown theme"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_unknown_flag_is_rejected() {
    let output = run(&["--nonsense"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("unknown option `--nonsense`"), "{text}");
    assert!(text.contains("USAGE"), "{text}");
}

#[test]
fn a_second_file_argument_is_rejected() {
    let scratch = Scratch::new("two-files");
    let a = scratch.write("a.md", "# A\n");
    let b = scratch.write("b.md", "# B\n");

    let output = run(&["--print-html", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("unexpected argument"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn print_html_without_a_file_explains_itself() {
    let output = run(&["--print-html"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("needs a file"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_directory_is_not_a_document() {
    let scratch = Scratch::new("isdir");
    let output = run(&["--print-html", scratch.0.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(!stderr(&output).contains("panicked"), "{}", stderr(&output));
}

#[test]
fn invalid_utf8_in_a_document_is_reported_not_fatal() {
    let scratch = Scratch::new("badutf8");
    let path = scratch.0.join("bad.md");
    fs::write(&path, [0xff, 0xfe, 0x00, 0x41]).expect("write bytes");

    let output = run(&["--print-html", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(!stderr(&output).contains("panicked"), "{}", stderr(&output));
}
