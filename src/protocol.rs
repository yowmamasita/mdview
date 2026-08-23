//! The `mdview` URL scheme.
//!
//! The webview never loads a `file://` URL. Instead every request goes through a
//! custom protocol whose path mirrors the filesystem, so that a relative link or
//! image inside a document resolves the way the author expected — `../img/a.png`
//! next to `/notes/deep/doc.md` becomes `/notes/img/a.png` by ordinary URL
//! resolution, with no rewriting on our side.
//!
//! One path prefix is reserved: [`crate::assets::ASSET_PREFIX`], which serves the
//! built-in stylesheet, viewer script and Mermaid runtime.

use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

use crate::assets;

/// Scheme name registered with the webview.
pub const SCHEME: &str = "mdview";

/// Characters escaped when a filesystem path becomes a URL path.
///
/// `/` and `:` are left alone: the first keeps path structure intact, the second
/// keeps Windows drive letters readable.
const PATH_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// What a webview request maps onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A built-in asset.
    Asset {
        body: &'static str,
        content_type: &'static str,
    },
    /// A Markdown document, to be rendered.
    Markdown(PathBuf),
    /// Any other local file, served as-is.
    File {
        path: PathBuf,
        content_type: &'static str,
    },
    /// Nothing to serve.
    NotFound,
}

/// The origin the webview sees.
///
/// Windows (and Android) cannot register a genuinely custom scheme, so wry maps
/// it onto an `http` subdomain; everywhere else the scheme is used directly.
pub fn origin() -> &'static str {
    if cfg!(any(windows, target_os = "android")) {
        "http://mdview.localhost"
    } else {
        "mdview://localhost"
    }
}

/// The URL that displays `path`.
pub fn document_url(path: &Path) -> String {
    format!("{}{}", origin(), url_path_for(path))
}

/// The URL path (leading `/`, percent-encoded) that stands for `path`.
pub fn url_path_for(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let raw = if raw.starts_with('/') {
        raw
    } else {
        format!("/{raw}")
    };
    utf8_percent_encode(&raw, PATH_SET).to_string()
}

/// Extract the decoded path component of a request URL.
///
/// Query and fragment are dropped; both `mdview://localhost/x` and
/// `http://mdview.localhost/x` are understood.
pub fn url_path(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://")?.1;
    let start = after_scheme.find('/')?;
    let path = &after_scheme[start..];
    let path = path
        .split_once(['?', '#'])
        .map_or(path, |(before, _)| before);
    Some(percent_decode_str(path).decode_utf8_lossy().into_owned())
}

/// Turn a decoded URL path into a filesystem path.
///
/// Returns `None` for the reserved asset prefix.
pub fn to_fs_path(url_path: &str) -> Option<PathBuf> {
    if url_path.starts_with(assets::ASSET_PREFIX) {
        return None;
    }
    // `/C:/Users/...` on Windows is really `C:\Users\...`.
    let trimmed = url_path.strip_prefix('/').unwrap_or(url_path);
    let is_drive = {
        let mut chars = trimmed.chars();
        matches!(
            (chars.next(), chars.next(), chars.next()),
            (Some(c), Some(':'), Some('/' | '\\')) if c.is_ascii_alphabetic()
        )
    };
    if is_drive {
        Some(PathBuf::from(trimmed))
    } else {
        Some(PathBuf::from(url_path))
    }
}

/// Resolve a request URL, consulting the filesystem for anything not built in.
pub fn resolve(url: &str) -> Resolved {
    let Some(path) = url_path(url) else {
        return Resolved::NotFound;
    };

    if let Some(name) = path.strip_prefix(assets::ASSET_PREFIX) {
        return match assets::get(name) {
            Some((body, content_type)) => Resolved::Asset { body, content_type },
            None => Resolved::NotFound,
        };
    }

    let Some(fs_path) = to_fs_path(&path) else {
        return Resolved::NotFound;
    };
    if !fs_path.is_file() {
        return Resolved::NotFound;
    }
    if crate::is_markdown_path(&fs_path) {
        Resolved::Markdown(fs_path)
    } else {
        let content_type = content_type(&fs_path);
        Resolved::File {
            path: fs_path,
            content_type,
        }
    }
}

/// Best-effort MIME type from a file extension.
pub fn content_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "txt" | "text" | "log" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}
