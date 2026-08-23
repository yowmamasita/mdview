//! The `mdview` URL scheme: turning webview requests into files and back.

use std::fs;
use std::path::{Path, PathBuf};

use mdview::assets;
use mdview::protocol::{
    Resolved, SCHEME, content_type, document_url, origin, resolve, to_fs_path, url_path,
    url_path_for,
};

/// A scratch directory that cleans itself up.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mdview-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// -------------------------------------------------------------------- urls

#[test]
fn the_scheme_and_origin_agree() {
    assert_eq!(SCHEME, "mdview");
    assert!(origin().contains("mdview"));
    assert!(document_url(Path::new("/a/b.md")).starts_with(origin()));
}

#[test]
fn a_path_round_trips_through_a_url() {
    for path in [
        "/tmp/notes.md",
        "/tmp/with space/notes.md",
        "/tmp/ünïcödé/notes.md",
        "/tmp/a+b/c&d.md",
        "/tmp/percent%20literal.md",
        "/tmp/hash#in-name.md",
        "/tmp/question?mark.md",
    ] {
        let url = document_url(Path::new(path));
        let decoded = url_path(&url).expect("a path component");
        assert_eq!(
            to_fs_path(&decoded).expect("a filesystem path"),
            PathBuf::from(path),
            "round trip failed for {path}"
        );
    }
}

#[test]
fn characters_that_would_change_a_url_are_escaped() {
    let encoded = url_path_for(Path::new("/tmp/a b#c?d.md"));
    assert!(!encoded.contains(' '), "{encoded}");
    assert!(!encoded.contains('#'), "{encoded}");
    assert!(!encoded.contains('?'), "{encoded}");
    // Slashes must survive, or relative resolution breaks.
    assert_eq!(encoded.matches('/').count(), 2, "{encoded}");
}

#[test]
fn query_and_fragment_are_not_part_of_the_path() {
    assert_eq!(
        url_path(&format!("{}/a/b.md#heading", origin())).as_deref(),
        Some("/a/b.md")
    );
    assert_eq!(
        url_path(&format!("{}/a/b.md?v=1", origin())).as_deref(),
        Some("/a/b.md")
    );
}

#[test]
fn both_url_shapes_are_understood() {
    // The scheme is used directly on macOS and Linux; Windows maps it onto an
    // http subdomain. Either must parse.
    assert_eq!(
        url_path("mdview://localhost/a/b.md").as_deref(),
        Some("/a/b.md")
    );
    assert_eq!(
        url_path("http://mdview.localhost/a/b.md").as_deref(),
        Some("/a/b.md")
    );
}

#[test]
fn malformed_urls_yield_nothing() {
    assert_eq!(url_path("not a url"), None);
    assert_eq!(url_path("mdview://localhost"), None);
    assert_eq!(url_path(""), None);
}

#[test]
fn a_windows_drive_path_loses_its_leading_slash() {
    assert_eq!(
        to_fs_path("/C:/Users/x/notes.md"),
        Some(PathBuf::from("C:/Users/x/notes.md"))
    );
    assert_eq!(
        to_fs_path("/z:/tmp/a.md"),
        Some(PathBuf::from("z:/tmp/a.md"))
    );
    // A single-letter directory is not a drive.
    assert_eq!(
        to_fs_path("/c/tmp/a.md"),
        Some(PathBuf::from("/c/tmp/a.md"))
    );
    assert_eq!(to_fs_path("/ab:/tmp"), Some(PathBuf::from("/ab:/tmp")));
}

#[test]
fn the_asset_prefix_is_never_a_filesystem_path() {
    assert_eq!(to_fs_path("/__mdview__/app.css"), None);
    assert_eq!(to_fs_path("/__mdview__/anything"), None);
    // Only an exact prefix match is reserved.
    assert!(to_fs_path("/__mdview__x/app.css").is_some());
}

#[test]
fn relative_links_resolve_against_the_document_path() {
    // This is why the URL path mirrors the filesystem: the webview does the
    // resolution itself, exactly as a browser would.
    let doc = document_url(Path::new("/notes/deep/doc.md"));
    let base = doc.rsplit_once('/').expect("a parent").0;
    let sibling = url_path(&format!("{base}/other.md")).expect("a path");
    assert_eq!(sibling, "/notes/deep/other.md");
}

// ---------------------------------------------------------------- resolving

#[test]
fn built_in_assets_resolve() {
    for (name, expected_type) in [
        ("app.css", "text/css; charset=utf-8"),
        ("app.js", "text/javascript; charset=utf-8"),
    ] {
        let url = format!("{}{}{}", origin(), assets::ASSET_PREFIX, name);
        match resolve(&url) {
            Resolved::Asset { body, content_type } => {
                assert!(!body.is_empty(), "{name} is empty");
                assert_eq!(content_type, expected_type);
            }
            other => panic!("{name} resolved to {other:?}"),
        }
    }
}

#[test]
fn the_mermaid_asset_follows_the_feature_flag() {
    let url = format!("{}{}mermaid.min.js", origin(), assets::ASSET_PREFIX);
    match (resolve(&url), assets::HAS_MERMAID) {
        (Resolved::Asset { body, .. }, true) => assert!(body.len() > 100_000),
        (Resolved::NotFound, false) => {}
        (other, has) => panic!("mermaid feature {has} resolved to {other:?}"),
    }
}

#[test]
fn an_unknown_asset_is_not_found() {
    let url = format!("{}{}nope.js", origin(), assets::ASSET_PREFIX);
    assert_eq!(resolve(&url), Resolved::NotFound);
}

#[test]
fn a_markdown_file_resolves_as_markdown() {
    let scratch = Scratch::new("markdown");
    let path = scratch.write("doc.md", "# Hi\n");
    match resolve(&document_url(&path)) {
        Resolved::Markdown(found) => assert_eq!(found, path),
        other => panic!("resolved to {other:?}"),
    }
}

#[test]
fn every_markdown_extension_is_recognised() {
    let scratch = Scratch::new("extensions");
    for ext in mdview::MARKDOWN_EXTENSIONS {
        let path = scratch.write(&format!("doc.{ext}"), "# Hi\n");
        assert!(
            matches!(resolve(&document_url(&path)), Resolved::Markdown(_)),
            ".{ext} was not treated as Markdown"
        );
    }
    // And the check is case-insensitive.
    let upper = scratch.write("DOC.MD", "# Hi\n");
    assert!(matches!(
        resolve(&document_url(&upper)),
        Resolved::Markdown(_)
    ));
}

#[test]
fn other_files_resolve_as_raw_bytes() {
    let scratch = Scratch::new("raw");
    let path = scratch.write("logo.png", "not really a png");
    match resolve(&document_url(&path)) {
        Resolved::File {
            path: found,
            content_type,
        } => {
            assert_eq!(found, path);
            assert_eq!(content_type, "image/png");
        }
        other => panic!("resolved to {other:?}"),
    }
}

#[test]
fn a_missing_file_is_not_found() {
    let url = document_url(Path::new("/definitely/not/here.md"));
    assert_eq!(resolve(&url), Resolved::NotFound);
}

#[test]
fn a_directory_is_not_served() {
    let scratch = Scratch::new("dir");
    scratch.write("sub/file.md", "# Hi\n");
    let url = document_url(&scratch.0.join("sub"));
    assert_eq!(resolve(&url), Resolved::NotFound);
}

#[test]
fn a_path_with_spaces_resolves() {
    let scratch = Scratch::new("spaces");
    let path = scratch.write("a document with spaces.md", "# Hi\n");
    assert!(matches!(
        resolve(&document_url(&path)),
        Resolved::Markdown(_)
    ));
}

// ------------------------------------------------------------ content types

#[test]
fn common_content_types_are_recognised() {
    for (name, expected) in [
        ("a.png", "image/png"),
        ("a.JPG", "image/jpeg"),
        ("a.jpeg", "image/jpeg"),
        ("a.gif", "image/gif"),
        ("a.svg", "image/svg+xml"),
        ("a.webp", "image/webp"),
        ("a.css", "text/css; charset=utf-8"),
        ("a.js", "text/javascript; charset=utf-8"),
        ("a.json", "application/json; charset=utf-8"),
        ("a.txt", "text/plain; charset=utf-8"),
        ("a.pdf", "application/pdf"),
        ("a.woff2", "font/woff2"),
        ("a.mp4", "video/mp4"),
    ] {
        assert_eq!(content_type(Path::new(name)), expected, "{name}");
    }
}

#[test]
fn unknown_extensions_fall_back_to_octet_stream() {
    assert_eq!(content_type(Path::new("a.qqq")), "application/octet-stream");
    assert_eq!(
        content_type(Path::new("noextension")),
        "application/octet-stream"
    );
}

#[test]
fn markdown_detection_matches_the_extension_list() {
    assert!(mdview::is_markdown_path(Path::new("a.md")));
    assert!(mdview::is_markdown_path(Path::new("a.MARKDOWN")));
    assert!(!mdview::is_markdown_path(Path::new("a.txt")));
    assert!(!mdview::is_markdown_path(Path::new("md")));
    assert!(!mdview::is_markdown_path(Path::new("a.md.bak")));
}
