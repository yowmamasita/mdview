//! Page assembly: themes, asset modes, the Content-Security-Policy and titles.

use std::str::FromStr;

use mdview::assets;
use mdview::document::{Assets, Document, Theme};
use mdview::markdown::{RenderOptions, render};

fn page(source: &str, mode: Assets) -> String {
    Document::from_markdown(source, &RenderOptions::viewer(), Theme::Auto, "/tmp/doc.md")
        .to_html(mode)
}

/// The `<head>` of a page, so assertions are not confused by the body.
fn head_of(html: &str) -> &str {
    &html[..html.find("</head>").expect("a head")]
}

/// The markup between `<body>` and the first `<script>`, so assertions are not
/// confused by an inlined stylesheet or runtime.
fn body_of(html: &str) -> &str {
    let start = html.find("<body>").expect("a body") + "<body>".len();
    let rest = &html[start..];
    let end = rest
        .find("<script")
        .unwrap_or_else(|| rest.find("</body>").expect("a body end"));
    &rest[..end]
}

// ------------------------------------------------------------------ themes

#[test]
fn themes_parse_from_their_names() {
    assert_eq!(Theme::from_str("auto").unwrap(), Theme::Auto);
    assert_eq!(Theme::from_str("system").unwrap(), Theme::Auto);
    assert_eq!(Theme::from_str("LIGHT").unwrap(), Theme::Light);
    assert_eq!(Theme::from_str(" dark ").unwrap(), Theme::Dark);
    assert_eq!(Theme::from_str("night").unwrap(), Theme::Dark);
    assert!(Theme::from_str("sepia").is_err());
    assert_eq!(Theme::default(), Theme::Auto);
}

#[test]
fn theme_names_round_trip() {
    for theme in [Theme::Auto, Theme::Light, Theme::Dark] {
        assert_eq!(Theme::from_str(theme.as_str()).unwrap(), theme);
        assert_eq!(theme.to_string(), theme.as_str());
    }
}

#[test]
fn toggling_settles_between_light_and_dark() {
    assert_eq!(Theme::Auto.toggled(), Theme::Dark);
    assert_eq!(Theme::Light.toggled(), Theme::Dark);
    assert_eq!(Theme::Dark.toggled(), Theme::Light);
    assert_eq!(Theme::Dark.toggled().toggled(), Theme::Dark);
}

#[test]
fn the_theme_preference_reaches_the_page() {
    for theme in [Theme::Auto, Theme::Light, Theme::Dark] {
        let html = Document::welcome(theme).to_html(Assets::Inline);
        assert!(
            html.contains(&format!(r#"data-theme-preference="{}""#, theme.as_str())),
            "{theme:?}"
        );
    }
}

// ------------------------------------------------------------------ assets

#[test]
fn linked_mode_references_assets_by_url() {
    let html = page("# Hi\n", Assets::Linked);
    assert!(
        html.contains(r#"<link rel="stylesheet" href="/__mdview__/app.css">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<script src="/__mdview__/app.js"></script>"#),
        "{html}"
    );
    assert!(
        !html.contains("--font-body"),
        "stylesheet must not be inlined"
    );
}

#[test]
fn inline_mode_embeds_everything() {
    let html = page("# Hi\n", Assets::Inline);
    assert!(html.contains("<style>"), "{}", &html[..400]);
    assert!(html.contains("--font-body"), "stylesheet must be inlined");
    assert!(html.contains("window.mdview"), "script must be inlined");
    assert!(!html.contains("/__mdview__/"), "no external references");
}

#[test]
fn the_mermaid_runtime_is_only_included_when_a_diagram_needs_it() {
    let without = page("# Hi\n", Assets::Linked);
    assert!(!without.contains("mermaid.min.js"), "{without}");

    let with = page("```mermaid\ngraph TD\nA-->B\n```\n", Assets::Linked);
    assert_eq!(
        with.contains("mermaid.min.js"),
        assets::HAS_MERMAID,
        "mermaid feature is {}",
        assets::HAS_MERMAID
    );
}

#[test]
fn inline_mode_embeds_the_mermaid_runtime() {
    if !assets::HAS_MERMAID {
        return;
    }
    let html = page("```mermaid\ngraph TD\nA-->B\n```\n", Assets::Inline);
    assert!(
        html.len() > assets::MERMAID_JS.len(),
        "runtime not embedded"
    );
    assert!(!html.contains("mermaid.min.js"), "must not also link it");
}

#[test]
fn the_bundled_mermaid_runtime_is_self_contained() {
    if !assets::HAS_MERMAID {
        return;
    }
    // The UMD build must not reach for anything at runtime: a dynamic import or
    // a remote URL would break under the page's Content-Security-Policy.
    assert!(assets::MERMAID_JS.contains("globalThis[\"mermaid\"]"));
    assert!(
        !assets::MERMAID_JS.contains("import("),
        "dynamic imports break offline use"
    );
}

// --------------------------------------------------------------------- CSP

#[test]
fn the_policy_forbids_network_access_and_framing() {
    for mode in [Assets::Linked, Assets::Inline] {
        let html = page("# Hi\n", mode);
        for directive in [
            "default-src &#39;none&#39;",
            "connect-src &#39;none&#39;",
            "base-uri &#39;none&#39;",
            "form-action &#39;none&#39;",
            "frame-ancestors &#39;none&#39;",
        ] {
            assert!(
                html.contains(directive),
                "{mode:?} missing {directive}: {html:.600}"
            );
        }
    }
}

#[test]
fn only_inline_mode_permits_inline_script() {
    let linked = page("# Hi\n", Assets::Linked);
    assert!(
        linked.contains("script-src &#39;self&#39;;"),
        "{linked:.700}"
    );

    let inline = page("# Hi\n", Assets::Inline);
    assert!(
        inline.contains("script-src &#39;unsafe-inline&#39;;"),
        "{inline:.700}"
    );
}

#[test]
fn styles_may_be_inline_because_mermaid_injects_them() {
    let html = page("# Hi\n", Assets::Linked);
    assert!(
        html.contains("style-src &#39;self&#39; &#39;unsafe-inline&#39;"),
        "{html:.700}"
    );
}

// ------------------------------------------------------------------ titles

#[test]
fn the_title_comes_from_the_first_heading() {
    let doc = Document::from_markdown(
        "# Real Title\n",
        &RenderOptions::viewer(),
        Theme::Auto,
        "/tmp/file.md",
    );
    assert_eq!(doc.title, "Real Title");
    assert!(
        doc.to_html(Assets::Inline)
            .contains("<title>Real Title</title>")
    );
}

#[test]
fn without_a_heading_the_file_name_is_used() {
    let doc = Document::from_markdown(
        "no heading\n",
        &RenderOptions::viewer(),
        Theme::Auto,
        "/a/b/notes.md",
    );
    assert_eq!(doc.title, "notes.md");
}

#[test]
fn titles_are_escaped() {
    let doc = Document::from_markdown(
        "# A < B & \"C\" </title><script>alert(1)</script>\n",
        &RenderOptions::viewer(),
        Theme::Auto,
        "/tmp/x.md",
    );
    let html = doc.to_html(Assets::Inline);
    let head = head_of(&html);
    assert!(head.contains("&lt;"), "{head}");
    assert!(head.contains("&amp;"), "{head}");
    assert!(head.contains("&quot;"), "{head}");
    assert!(!head.contains("</title><script"), "{head}");
    assert_eq!(head.matches("<title>").count(), 1, "{head}");
}

#[test]
fn the_document_key_is_escaped_too() {
    let doc = Document::welcome(Theme::Auto);
    let quoted = Document {
        key: r#"a" onload="alert(1)"#.to_string(),
        ..doc
    };
    let html = quoted.to_html(Assets::Inline);
    let head = head_of(&html);
    assert!(!head.contains(r#"onload="alert(1)""#), "{head}");
    assert!(head.contains("&quot;"), "{head}");
}

// ------------------------------------------------------------- page shapes

#[test]
fn the_welcome_page_explains_how_to_open_a_file() {
    let html = Document::welcome(Theme::Auto).to_html(Assets::Inline);
    let body = body_of(&html);
    assert!(body.contains("md-empty"), "{body}");
    assert!(body.contains("mdview"), "{body}");
    assert!(
        !body.contains("md-body"),
        "no article wrapper when empty: {body}"
    );
}

#[test]
fn the_error_page_reports_the_problem_without_injecting_it() {
    let html = Document::error("<img src=x onerror=alert(1)>", Theme::Dark).to_html(Assets::Inline);
    let body = body_of(&html);
    assert!(body.contains("Could not open document"), "{body}");
    assert!(!body.contains("<img"), "{body}");
    assert!(body.contains("&lt;img"), "{body}");
}

#[test]
fn a_rendered_page_is_well_formed() {
    let html = page(include_str!("fixtures/kitchen-sink.md"), Assets::Linked);
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.trim_end().ends_with("</html>"));
    assert_eq!(html.matches("<body>").count(), 1);
    assert_eq!(html.matches("</html>").count(), 1);
    assert!(html.contains(r#"<meta charset="utf-8">"#));
    assert!(html.contains("<article class=\"md-body\">"));
}

#[test]
fn a_document_can_be_rendered_twice_identically() {
    let doc = Document::from_markdown(
        include_str!("fixtures/kitchen-sink.md"),
        &RenderOptions::viewer(),
        Theme::Light,
        "/tmp/k.md",
    );
    assert_eq!(doc.to_html(Assets::Linked), doc.to_html(Assets::Linked));
}

#[test]
fn from_rendered_matches_from_markdown() {
    let source = "# Heading\n\ntext\n";
    let opts = RenderOptions::viewer();
    let direct = Document::from_markdown(source, &opts, Theme::Dark, "/k.md");
    let staged = Document::from_rendered(render(source, &opts), Theme::Dark, "/k.md");
    assert_eq!(direct, staged);
}
