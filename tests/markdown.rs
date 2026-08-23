//! Rendering behaviour beyond raw specification conformance: Mermaid blocks,
//! heading anchors, GFM markup, front matter, and the metadata `render` returns.

use mdview::markdown::{Flavor, RenderOptions, Rendered, render, slugify};

/// Render with the viewer's own settings, sanitizer included.
fn viewer(source: &str) -> Rendered {
    render(source, &RenderOptions::viewer())
}

/// Render with GFM extensions but no sanitizer, to inspect raw output.
fn raw_gfm(source: &str) -> String {
    let mut opts = RenderOptions::viewer();
    opts.sanitize = false;
    render(source, &opts).html
}

// ----------------------------------------------------------------- mermaid

#[test]
fn a_mermaid_fence_becomes_a_mermaid_block() {
    let out = viewer("```mermaid\ngraph TD\nA-->B\n```\n");
    assert!(
        out.html.contains(r#"<pre class="mermaid">"#),
        "{}",
        out.html
    );
    assert!(out.html.contains("graph TD\nA--&gt;B"), "{}", out.html);
    assert_eq!(out.mermaid_blocks, 1);
    assert!(out.has_mermaid());
}

#[test]
fn mermaid_source_is_escaped_not_executed() {
    let out = viewer("```mermaid\ngraph TD\nA[\"</pre><script>alert(1)</script>\"]\n```\n");
    assert!(!out.html.contains("<script>"), "{}", out.html);
    assert!(out.html.contains("&lt;script&gt;"), "{}", out.html);
    // The block is still one element, not truncated by the injected close tag.
    assert_eq!(out.html.matches("<pre").count(), 1, "{}", out.html);
}

#[test]
fn the_mermaid_info_string_is_matched_case_insensitively_and_by_word() {
    assert_eq!(viewer("```Mermaid\na\n```\n").mermaid_blocks, 1);
    assert_eq!(viewer("```mermaid theme=dark\na\n```\n").mermaid_blocks, 1);
    assert_eq!(viewer("```mermaidish\na\n```\n").mermaid_blocks, 0);
    assert_eq!(viewer("```rust\na\n```\n").mermaid_blocks, 0);
}

#[test]
fn mermaid_can_be_turned_off() {
    let mut opts = RenderOptions::viewer();
    opts.mermaid = false;
    let out = render("```mermaid\ngraph TD\n```\n", &opts);
    assert_eq!(out.mermaid_blocks, 0);
    assert!(!out.has_mermaid());
    assert!(out.html.contains("language-mermaid"), "{}", out.html);
}

#[test]
fn several_mermaid_blocks_are_all_counted() {
    let source = "```mermaid\na\n```\n\ntext\n\n```mermaid\nb\n```\n";
    assert_eq!(viewer(source).mermaid_blocks, 2);
}

#[test]
fn an_empty_mermaid_fence_is_still_a_block() {
    let out = viewer("```mermaid\n```\n");
    assert_eq!(out.mermaid_blocks, 1);
    assert!(
        out.html.contains(r#"<pre class="mermaid"></pre>"#),
        "{}",
        out.html
    );
}

// ---------------------------------------------------------------- headings

#[test]
fn headings_get_github_style_slugs() {
    let out = viewer("# Hello, World!\n## A *nested* heading\n");
    assert!(
        out.html.contains(r#"<h1 id="hello-world">"#),
        "{}",
        out.html
    );
    assert!(
        out.html.contains(r#"<h2 id="a-nested-heading">"#),
        "{}",
        out.html
    );
}

#[test]
fn duplicate_headings_get_distinct_ids() {
    let out = viewer("# Setup\n# Setup\n# Setup\n");
    assert!(out.html.contains(r#"id="setup""#));
    assert!(out.html.contains(r#"id="setup-1""#));
    assert!(out.html.contains(r#"id="setup-2""#));
}

#[test]
fn headings_are_collected_in_document_order() {
    let out = viewer("# One\n### Three\n## Two\n");
    let levels: Vec<u8> = out.headings.iter().map(|h| h.level).collect();
    let texts: Vec<&str> = out.headings.iter().map(|h| h.text.as_str()).collect();
    assert_eq!(levels, [1, 3, 2]);
    assert_eq!(texts, ["One", "Three", "Two"]);
}

#[test]
fn heading_text_includes_code_spans_but_not_markup() {
    let out = viewer("## Using `render()` **now**\n");
    assert_eq!(out.headings[0].text, "Using render() now");
    assert_eq!(out.headings[0].id, "using-render-now");
}

#[test]
fn the_title_is_the_first_level_one_heading() {
    assert_eq!(viewer("# Title\n# Other\n").title.as_deref(), Some("Title"));
    assert_eq!(viewer("## Only h2\n").title, None);
    assert_eq!(viewer("no headings\n").title, None);
}

#[test]
fn an_empty_heading_still_gets_an_id() {
    let out = viewer("#\n");
    assert_eq!(out.headings[0].id, "section");
}

#[test]
fn heading_ids_can_be_turned_off() {
    let mut opts = RenderOptions::viewer();
    opts.heading_ids = false;
    let out = render("# Hello\n", &opts);
    assert!(out.headings.is_empty());
    assert!(!out.html.contains("id="), "{}", out.html);
}

#[test]
fn slugify_matches_github_conventions() {
    assert_eq!(slugify("Hello, World!"), "hello-world");
    assert_eq!(slugify("  spaced  out  "), "spaced--out");
    assert_eq!(slugify("Ünïcödé Ok"), "ünïcödé-ok");
    assert_eq!(slugify("under_score-and-dash"), "under_score-and-dash");
    assert_eq!(slugify("100% done"), "100-done");
    assert_eq!(slugify(""), "");
}

// ------------------------------------------------------------- gfm markup

#[test]
fn alerts_become_titled_callouts() {
    let out = viewer("> [!NOTE]\n> Something.\n");
    assert!(
        out.html
            .contains(r#"<div class="markdown-alert markdown-alert-note">"#),
        "{}",
        out.html
    );
    assert!(
        out.html
            .contains(r#"<p class="markdown-alert-title">Note</p>"#),
        "{}",
        out.html
    );
    assert!(out.html.contains("</div>"), "{}", out.html);
}

#[test]
fn every_alert_kind_is_recognised() {
    for (marker, class, title) in [
        ("NOTE", "note", "Note"),
        ("TIP", "tip", "Tip"),
        ("IMPORTANT", "important", "Important"),
        ("WARNING", "warning", "Warning"),
        ("CAUTION", "caution", "Caution"),
    ] {
        let out = viewer(&format!("> [!{marker}]\n> body\n"));
        assert!(
            out.html.contains(&format!("markdown-alert-{class}")),
            "{marker}: {}",
            out.html
        );
        assert!(
            out.html.contains(&format!(">{title}</p>")),
            "{marker}: {}",
            out.html
        );
    }
}

#[test]
fn an_ordinary_blockquote_is_untouched() {
    let out = viewer("> just a quote\n");
    assert!(out.html.contains("<blockquote>"), "{}", out.html);
    assert!(!out.html.contains("markdown-alert"), "{}", out.html);
}

#[test]
fn nested_blockquotes_inside_an_alert_close_correctly() {
    let html = raw_gfm("> [!TIP]\n> outer\n>\n> > inner\n");
    assert_eq!(
        html.matches("<div class=\"markdown-alert").count(),
        1,
        "{html}"
    );
    assert_eq!(html.matches("</div>").count(), 1, "{html}");
    assert_eq!(html.matches("<blockquote>").count(), 1, "{html}");
    assert_eq!(html.matches("</blockquote>").count(), 1, "{html}");
}

#[test]
fn task_list_items_are_tagged_and_inert() {
    let out = viewer("- [x] done\n- [ ] todo\n");
    assert_eq!(
        out.html.matches(r#"<li class="task-list-item">"#).count(),
        2
    );
    assert!(
        out.html
            .contains(r#"<input checked="" disabled="" type="checkbox">"#),
        "{}",
        out.html
    );
    assert_eq!(out.html.matches("disabled").count(), 2, "{}", out.html);
}

#[test]
fn a_list_item_without_a_marker_is_not_a_task_item() {
    let out = viewer("- plain\n");
    assert!(!out.html.contains("task-list-item"), "{}", out.html);
}

#[test]
fn tables_strikethrough_and_footnotes_render() {
    let out = viewer("| a | b |\n| - | - |\n| 1 | 2 |\n\n~~gone~~\n\nnote[^1]\n\n[^1]: body\n");
    assert!(out.html.contains("<table>"), "{}", out.html);
    assert!(out.html.contains("<del>gone</del>"), "{}", out.html);
    assert!(out.html.contains("footnote-reference"), "{}", out.html);
}

#[test]
fn gfm_features_are_absent_in_commonmark_mode() {
    let opts = RenderOptions::commonmark();
    let out = render(
        "| a | b |\n| - | - |\n\n~~gone~~\n\nwww.example.com\n",
        &opts,
    );
    assert!(!out.html.contains("<table>"), "{}", out.html);
    assert!(!out.html.contains("<del>"), "{}", out.html);
    assert!(!out.html.contains("<a href"), "{}", out.html);
    assert_eq!(opts.flavor, Flavor::CommonMark);
}

// ------------------------------------------------------------- autolinking

#[test]
fn bare_urls_become_links_in_gfm_mode() {
    let out = viewer("Visit www.example.com or https://example.org today.\n");
    assert!(
        out.html.contains(r#"<a href="http://www.example.com""#),
        "{}",
        out.html
    );
    assert!(
        out.html.contains(r#"<a href="https://example.org""#),
        "{}",
        out.html
    );
}

#[test]
fn urls_inside_code_are_not_linked() {
    let out = viewer("```\nhttps://example.org\n```\n\n`https://example.org`\n");
    assert!(!out.html.contains("<a href"), "{}", out.html);
}

#[test]
fn urls_inside_an_existing_link_are_not_linked_again() {
    let out = viewer("[see https://example.org here](/local)\n");
    assert_eq!(out.html.matches("<a ").count(), 1, "{}", out.html);
}

#[test]
fn a_link_split_across_text_events_is_scanned_as_a_whole() {
    // `pulldown-cmark` breaks the run at `_`; without re-joining it, the
    // trailing underscore would be invisible and the address would linkify.
    let out = viewer("a.b-c_d@a.b_\n");
    assert!(!out.html.contains("<a href"), "{}", out.html);
}

// ------------------------------------------------------------ front matter

#[test]
fn yaml_front_matter_is_hidden() {
    let out = viewer("---\ntitle: Test\ntags: [a, b]\n---\n\n# Body\n");
    assert!(!out.html.contains("title: Test"), "{}", out.html);
    assert!(out.html.contains("<h1"), "{}", out.html);
    assert_eq!(out.title.as_deref(), Some("Body"));
}

#[test]
fn front_matter_handling_can_be_turned_off() {
    let mut opts = RenderOptions::viewer();
    opts.front_matter = false;
    let out = render("---\ntitle: Test\n---\n\n# Body\n", &opts);
    assert!(out.html.contains("title: Test"), "{}", out.html);
}

#[test]
fn a_lone_thematic_break_is_not_mistaken_for_front_matter() {
    let out = viewer("---\n\ntext\n");
    assert!(out.html.contains("<hr"), "{}", out.html);
}

// ---------------------------------------------------------------- general

#[test]
fn rendering_is_deterministic() {
    let source = include_str!("fixtures/kitchen-sink.md");
    assert_eq!(viewer(source).html, viewer(source).html);
}

#[test]
fn an_empty_document_renders_to_nothing() {
    let out = viewer("");
    assert_eq!(out.html, "");
    assert_eq!(out.title, None);
    assert!(out.headings.is_empty());
    assert_eq!(out.mermaid_blocks, 0);
}

#[test]
fn text_with_no_trailing_newline_still_renders() {
    assert!(viewer("# no newline").html.contains("<h1"));
}

#[test]
fn very_deep_nesting_does_not_blow_the_stack() {
    let deep = ">".repeat(2000) + " deep\n";
    let _ = viewer(&deep);
    let brackets = "[".repeat(5000);
    let _ = viewer(&brackets);
}

#[test]
fn the_kitchen_sink_fixture_exercises_every_feature() {
    let out = viewer(include_str!("fixtures/kitchen-sink.md"));
    assert!(out.has_mermaid());
    assert!(out.title.is_some());
    assert!(out.headings.len() > 5);
    assert!(out.html.contains("<table>"));
    assert!(out.html.contains("task-list-item"));
    assert!(out.html.contains("markdown-alert"));
    assert!(out.html.contains("<del>"));
    assert!(!out.html.contains("<script"));
}
