//! Assembling a standalone HTML page around a rendered Markdown fragment.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use crate::assets;
use crate::markdown::{self, RenderOptions, Rendered, escape_html};

/// Colour scheme for the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    Auto,
    Light,
    Dark,
}

impl Theme {
    /// The value written to `data-theme-preference`.
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    /// Cycle light → dark → light. `Auto` resolves to `Light` first.
    pub fn toggled(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            _ => Theme::Dark,
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Theme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "system" => Ok(Theme::Auto),
            "light" | "day" => Ok(Theme::Light),
            "dark" | "night" => Ok(Theme::Dark),
            other => Err(format!(
                "unknown theme `{other}` (expected auto, light or dark)"
            )),
        }
    }
}

/// How the page should reference the stylesheet, viewer script and Mermaid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assets {
    /// Reference them through the `mdview` protocol — used by the window.
    Linked,
    /// Embed them, producing a page that stands on its own.
    Inline,
}

/// A complete page ready to hand to a webview or write to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Contents of `<title>`.
    pub title: String,
    /// Sanitized HTML fragment for the document body.
    pub body: String,
    /// Colour scheme preference.
    pub theme: Theme,
    /// Stable key for the page, used to restore scroll position on reload.
    pub key: String,
    /// Whether the page needs the Mermaid runtime.
    pub mermaid: bool,
}

impl Document {
    /// Render `source` and wrap it in a page.
    pub fn from_markdown(source: &str, opts: &RenderOptions, theme: Theme, key: &str) -> Self {
        let rendered = markdown::render(source, opts);
        Self::from_rendered(rendered, theme, key)
    }

    /// Wrap an already-rendered fragment in a page.
    pub fn from_rendered(rendered: Rendered, theme: Theme, key: &str) -> Self {
        let mermaid = rendered.has_mermaid() && assets::HAS_MERMAID;
        let title = rendered
            .title
            .filter(|t| !t.trim().is_empty())
            .or_else(|| title_from_key(key))
            .unwrap_or_else(|| "mdview".to_string());

        Document {
            title,
            body: rendered.html,
            theme,
            key: key.to_string(),
            mermaid,
        }
    }

    /// The "no document open" page.
    pub fn welcome(theme: Theme) -> Self {
        Document {
            title: "mdview".to_string(),
            body: String::new(),
            theme,
            key: "welcome".to_string(),
            mermaid: false,
        }
    }

    /// A page describing why a document could not be shown.
    pub fn error(message: &str, theme: Theme) -> Self {
        Document {
            title: "mdview — error".to_string(),
            body: format!(
                "<div class=\"md-empty\"><h1>Could not open document</h1><p>{}</p></div>",
                escape_html(message)
            ),
            theme,
            key: "error".to_string(),
            mermaid: false,
        }
    }

    /// Serialize the page.
    pub fn to_html(&self, mode: Assets) -> String {
        let mut out = String::with_capacity(self.body.len() + assets::APP_CSS.len() + 4096);

        out.push_str("<!doctype html>\n<html lang=\"en\" data-theme-preference=\"");
        out.push_str(self.theme.as_str());
        out.push_str("\" data-doc=\"");
        out.push_str(&escape_html(&self.key));
        out.push_str("\">\n<head>\n<meta charset=\"utf-8\">\n");
        out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        out.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"");
        out.push_str(&escape_html(&content_security_policy(mode)));
        out.push_str("\">\n<title>");
        out.push_str(&escape_html(&self.title));
        out.push_str("</title>\n");

        match mode {
            Assets::Linked => {
                out.push_str("<link rel=\"stylesheet\" href=\"");
                out.push_str(&assets::url("app.css"));
                out.push_str("\">\n");
            }
            Assets::Inline => {
                out.push_str("<style>\n");
                out.push_str(assets::APP_CSS);
                out.push_str("\n</style>\n");
            }
        }

        out.push_str("</head>\n<body>\n");
        if self.body.trim().is_empty() {
            out.push_str(WELCOME_BODY);
        } else {
            out.push_str("<main class=\"md-page\">\n<article class=\"md-body\">\n");
            out.push_str(&self.body);
            out.push_str("\n</article>\n</main>\n");
        }

        if self.mermaid {
            match mode {
                Assets::Linked => {
                    out.push_str("<script src=\"");
                    out.push_str(&assets::url("mermaid.min.js"));
                    out.push_str("\"></script>\n");
                }
                Assets::Inline => {
                    out.push_str("<script>\n");
                    out.push_str(assets::MERMAID_JS);
                    out.push_str("\n</script>\n");
                }
            }
        }

        match mode {
            Assets::Linked => {
                out.push_str("<script src=\"");
                out.push_str(&assets::url("app.js"));
                out.push_str("\"></script>\n");
            }
            Assets::Inline => {
                out.push_str("<script>\n");
                out.push_str(assets::APP_JS);
                out.push_str("\n</script>\n");
            }
        }

        out.push_str("</body>\n</html>\n");
        out
    }
}

/// Shown when no document is open.
const WELCOME_BODY: &str = "<div class=\"md-empty\">\n<h1>mdview</h1>\n\
<p>Drop a Markdown file here, or press <kbd>\u{2318}O</kbd> / <kbd>Ctrl+O</kbd> to open one.</p>\n\
</div>\n";

/// The page's Content-Security-Policy.
///
/// Documents are sanitized before they get here; this is the second wall. Note
/// that `style-src` must allow inline styles because Mermaid injects them at
/// runtime, and `connect-src 'none'` is what keeps a document from phoning home.
fn content_security_policy(mode: Assets) -> String {
    let script_src = match mode {
        Assets::Linked => "'self'",
        Assets::Inline => "'unsafe-inline'",
    };
    format!(
        "default-src 'none'; \
         script-src {script_src}; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         font-src 'self' data:; \
         media-src 'self'; \
         connect-src 'none'; \
         base-uri 'none'; \
         form-action 'none'; \
         frame-ancestors 'none'"
    )
}

/// Fall back to the file name when a document has no level-1 heading.
fn title_from_key(key: &str) -> Option<String> {
    Path::new(key)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .filter(|n| !n.is_empty())
}
