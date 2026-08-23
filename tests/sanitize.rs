//! The HTML allowlist.
//!
//! A viewer is routinely pointed at files from elsewhere, and Markdown may embed
//! arbitrary HTML. Everything the renderer produces passes through the sanitizer
//! before it reaches the webview; these tests fix what survives that pass and
//! what does not.

use mdview::markdown::{RenderOptions, render};
use mdview::sanitize::clean;

/// Render the way the viewer does, sanitizer included.
fn viewer(source: &str) -> String {
    render(source, &RenderOptions::viewer()).html
}

// ------------------------------------------------------------------ scripts

#[test]
fn script_elements_never_survive() {
    for source in [
        "<script>alert(1)</script>",
        "<SCRIPT>alert(1)</SCRIPT>",
        "<script src=\"https://evil.example/x.js\"></script>",
        "<script\ntype=\"module\">alert(1)</script>",
        "<div><script>alert(1)</script></div>",
    ] {
        let html = viewer(source);
        assert!(
            !html.to_ascii_lowercase().contains("<script"),
            "{source:?} produced {html:?}"
        );
    }
}

#[test]
fn event_handler_attributes_are_stripped() {
    for source in [
        r#"<img src="x" onerror="alert(1)">"#,
        r#"<div onclick="alert(1)">click</div>"#,
        r#"<body onload="alert(1)">"#,
        r#"<svg onload="alert(1)"></svg>"#,
    ] {
        let html = viewer(source);
        assert!(!html.contains("onerror"), "{source:?} -> {html:?}");
        assert!(!html.contains("onclick"), "{source:?} -> {html:?}");
        assert!(!html.contains("onload"), "{source:?} -> {html:?}");
    }
}

#[test]
fn javascript_urls_are_dropped() {
    for source in [
        "[x](javascript:alert(1))",
        "[x](JaVaScRiPt:alert(1))",
        "[x](  javascript:alert(1))",
        "[x](vbscript:msgbox)",
        r#"<a href="javascript:alert(1)">x</a>"#,
    ] {
        let html = viewer(source);
        assert!(
            !html.to_ascii_lowercase().contains("javascript:"),
            "{source:?} -> {html:?}"
        );
        assert!(
            !html.to_ascii_lowercase().contains("vbscript:"),
            "{source:?} -> {html:?}"
        );
    }
}

#[test]
fn data_urls_are_allowed_for_images_but_not_for_links() {
    let img = viewer("![x](data:image/png;base64,iVBORw0KGgo=)");
    assert!(img.contains("data:image/png"), "{img}");

    let link = viewer("[x](data:text/html,<script>alert(1)</script>)");
    assert!(!link.contains("data:text/html"), "{link}");
}

#[test]
fn dangerous_embedding_elements_are_removed() {
    for tag in [
        "iframe", "object", "embed", "form", "style", "base", "meta", "link", "svg",
    ] {
        let html = viewer(&format!("<{tag}>"));
        assert!(
            !html.contains(&format!("<{tag}")),
            "<{tag}> survived: {html:?}"
        );
    }
}

#[test]
fn a_form_field_cannot_be_smuggled_in_as_a_task_checkbox() {
    // Whatever an `<input>` claims to be, it comes out a disabled checkbox.
    for source in [
        r#"<input type="text" name="password" value="x">"#,
        r#"<input type="submit" formaction="https://evil.example">"#,
        "<input>",
    ] {
        let html = clean(source);
        assert!(html.contains(r#"type="checkbox""#), "{source:?} -> {html}");
        assert!(html.contains(r#"disabled="""#), "{source:?} -> {html}");
        assert!(!html.contains("name="), "{source:?} -> {html}");
        assert!(!html.contains("value="), "{source:?} -> {html}");
        assert!(!html.contains("formaction"), "{source:?} -> {html}");
    }
}

// ----------------------------------------------------------------- classes

#[test]
fn only_classes_the_renderer_emits_survive() {
    assert!(clean(r#"<pre class="mermaid">x</pre>"#).contains(r#"class="mermaid""#));
    assert!(!clean(r#"<pre class="evil">x</pre>"#).contains("class"));
    assert!(clean(r#"<code class="language-rust">x</code>"#).contains("language-rust"));
    assert!(!clean(r#"<code class="language- rust">x</code>"#).contains("language-"));
    assert!(clean(r#"<div class="markdown-alert">x</div>"#).contains("markdown-alert"));
    assert!(!clean(r#"<span class="tooltip">x</span>"#).contains("class"));
}

#[test]
fn a_mixed_class_list_keeps_only_the_permitted_entries() {
    let html = clean(r#"<div class="evil markdown-alert other">x</div>"#);
    assert!(html.contains(r#"class="markdown-alert""#), "{html}");
    assert!(!html.contains("evil"), "{html}");
}

#[test]
fn renderer_output_keeps_the_classes_the_stylesheet_needs() {
    let html = viewer(
        "```mermaid\na\n```\n\n```rust\nb\n```\n\n> [!TIP]\n> t\n\n- [x] done\n\nnote[^1]\n\n[^1]: body\n",
    );
    for class in [
        "mermaid",
        "language-rust",
        "markdown-alert",
        "markdown-alert-tip",
        "markdown-alert-title",
        "task-list-item",
        "footnote-reference",
    ] {
        assert!(html.contains(class), "missing {class}: {html}");
    }
}

// ------------------------------------------------------------------ styles

/// A `<td>` only survives HTML parsing inside a table, so wrap it in one.
fn cell(style: &str) -> String {
    clean(&format!(
        r#"<table><tbody><tr><td style="{style}">x</td></tr></tbody></table>"#
    ))
}

#[test]
fn only_table_alignment_survives_as_an_inline_style() {
    for alignment in ["left", "center", "right"] {
        let html = cell(&format!("text-align: {alignment}"));
        assert!(html.contains(&format!("text-align: {alignment}")), "{html}");
    }
    for style in [
        "position: fixed; top: 0",
        "background: url(https://evil.example/x)",
        "text-align: left; position: fixed",
        "behavior: url(#x)",
        "text-align: justify",
        "TEXT-ALIGN: left; x: y",
    ] {
        let html = cell(style);
        assert!(!html.contains("style="), "{style:?} survived: {html}");
    }
}

#[test]
fn table_alignment_from_the_renderer_is_preserved() {
    let html = viewer("| a | b |\n| :- | -: |\n| 1 | 2 |\n");
    assert!(html.contains("text-align: left"), "{html}");
    assert!(html.contains("text-align: right"), "{html}");
}

// ------------------------------------------------------------------ markup

#[test]
fn ordinary_formatting_is_preserved() {
    let html = viewer(
        "# H\n\ntext with *em*, **strong**, `code`, ~~del~~\n\n\
         | a |\n| - |\n| 1 |\n\n> quote\n\n- item\n\n![alt](./a.png)\n\n[link](./b.md)\n",
    );
    for fragment in [
        "<h1",
        "<em>",
        "<strong>",
        "<code>",
        "<del>",
        "<table>",
        "<blockquote>",
        "<li>",
        "<img",
        "<a href=\"./b.md\"",
    ] {
        assert!(html.contains(fragment), "missing {fragment}: {html}");
    }
}

#[test]
fn relative_urls_are_left_alone() {
    let html = viewer("[a](./sibling.md) ![b](../img/x.png) [c](#anchor)\n");
    assert!(html.contains(r#"href="./sibling.md""#), "{html}");
    assert!(html.contains(r#"src="../img/x.png""#), "{html}");
    assert!(html.contains(r##"href="#anchor""##), "{html}");
}

#[test]
fn links_carry_a_conservative_rel() {
    let html = viewer("[a](https://example.com)\n");
    assert!(html.contains(r#"rel="noopener noreferrer""#), "{html}");
}

#[test]
fn heading_and_footnote_ids_are_kept() {
    let html = viewer("# Title\n\nnote[^1]\n\n[^1]: body\n");
    assert!(html.contains(r#"id="title""#), "{html}");
    assert!(html.contains("id=\"1\""), "{html}");
}

#[test]
fn comments_and_unknown_elements_are_discarded() {
    let html = clean("<!-- secret --><custom-element>x</custom-element><marquee>y</marquee>");
    assert!(!html.contains("secret"), "{html}");
    assert!(!html.contains("<custom-element"), "{html}");
    assert!(!html.contains("<marquee"), "{html}");
    // Text content is kept even when its wrapper is not.
    assert!(html.contains('x') && html.contains('y'), "{html}");
}

#[test]
fn sanitizing_is_idempotent() {
    let once = viewer(include_str!("fixtures/kitchen-sink.md"));
    assert_eq!(clean(&once), once);
}

#[test]
fn sanitizing_can_be_turned_off_for_trusted_input() {
    let mut opts = RenderOptions::viewer();
    opts.sanitize = false;
    let html = render("<div class=\"anything\">x</div>", &opts).html;
    assert!(html.contains(r#"class="anything""#), "{html}");
}

// -------------------------------------------------------- gfm tag filtering

#[test]
fn gfm_tagfilter_escapes_disallowed_tags_before_sanitizing() {
    let mut opts = RenderOptions::viewer();
    opts.sanitize = false;
    let html = render("<strong> <title> <style> <em>\n", &opts).html;
    assert!(html.contains("&lt;title>"), "{html}");
    assert!(html.contains("&lt;style>"), "{html}");
    assert!(html.contains("<strong>"), "{html}");
    assert!(html.contains("<em>"), "{html}");
}

#[test]
fn tagfilter_is_case_insensitive_and_covers_closing_tags() {
    let mut opts = RenderOptions::viewer();
    opts.sanitize = false;
    let html = render("<XMP> is disallowed. </xmp> too.\n", &opts).html;
    assert!(html.contains("&lt;XMP>"), "{html}");
    assert!(html.contains("&lt;/xmp>"), "{html}");
}

#[test]
fn tagfilter_leaves_lookalike_names_alone() {
    let mut opts = RenderOptions::viewer();
    opts.sanitize = false;
    let html = render("<scriptish> <titles> <style-x>\n", &opts).html;
    assert!(!html.contains("&lt;scriptish"), "{html}");
    assert!(!html.contains("&lt;titles"), "{html}");
    assert!(!html.contains("&lt;style-x"), "{html}");
}
