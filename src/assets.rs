//! Vendored static assets, compiled into the binary.
//!
//! Nothing is fetched at runtime: a document renders identically with the
//! machine offline, which is the whole point of a local viewer.

/// Stylesheet applied to every rendered document.
pub const APP_CSS: &str = include_str!("../assets/app.css");

/// Viewer runtime: theme, zoom, Mermaid, scroll restoration, shortcuts.
pub const APP_JS: &str = include_str!("../assets/app.js");

/// Version of the bundled Mermaid runtime.
pub const MERMAID_VERSION: &str = "11.17.0";

/// The bundled Mermaid runtime (UMD build, no dynamic imports).
#[cfg(feature = "mermaid")]
pub const MERMAID_JS: &str = include_str!("../assets/mermaid.min.js");

/// The bundled Mermaid runtime — absent from this build.
#[cfg(not(feature = "mermaid"))]
pub const MERMAID_JS: &str = "";

/// Whether this build can render Mermaid diagrams.
pub const HAS_MERMAID: bool = cfg!(feature = "mermaid");

/// URL prefix reserved for assets; never mistaken for a filesystem path.
pub const ASSET_PREFIX: &str = "/__mdview__/";

/// Look up a built-in asset by its name under [`ASSET_PREFIX`].
pub fn get(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "app.css" => Some((APP_CSS, "text/css; charset=utf-8")),
        "app.js" => Some((APP_JS, "text/javascript; charset=utf-8")),
        "mermaid.min.js" if HAS_MERMAID => Some((MERMAID_JS, "text/javascript; charset=utf-8")),
        _ => None,
    }
}

/// The in-page URL for a built-in asset.
pub fn url(name: &str) -> String {
    format!("{ASSET_PREFIX}{name}")
}
