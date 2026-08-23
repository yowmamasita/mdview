//! Tests for the conformance harness itself.
//!
//! The CommonMark and GFM suites only mean something if the specification
//! parser and the HTML normaliser behind them are right, so both are pinned
//! here — the normaliser against the doctests of the `normalize.py` it is a port
//! of, and the parsers against the shipped specification files.

mod common;

use common::html::normalize;
use common::{Comparison, parse_spec_json, parse_spec_txt};

const COMMONMARK_SPEC: &str = include_str!("spec/commonmark-0.31.2.json");
const GFM_SPEC: &str = include_str!("spec/gfm-spec.txt");

// ------------------------------------------------- normalizer (normalize.py)

#[test]
fn inner_whitespace_collapses() {
    assert_eq!(normalize("<p>a  \t b</p>"), "<p>a b</p>");
    assert_eq!(normalize("<p>a  \t\nb</p>"), "<p>a b</p>");
}

#[test]
fn whitespace_around_block_tags_is_removed() {
    assert_eq!(normalize("<p>a  b</p>"), "<p>a b</p>");
    assert_eq!(normalize(" <p>a  b</p>"), "<p>a b</p>");
    assert_eq!(normalize("<p>a  b</p> "), "<p>a b</p>");
    assert_eq!(normalize("\n\t<p>\n\t\ta  b\t\t</p>\n\t"), "<p>a b</p>");
}

#[test]
fn whitespace_around_inline_tags_is_kept() {
    assert_eq!(normalize("<i>a  b</i> "), "<i>a b</i> ");
}

#[test]
fn whitespace_inside_pre_is_preserved() {
    assert_eq!(
        normalize("<pre><code>a  \n  b\n</code></pre>"),
        "<pre><code>a  \n  b\n</code></pre>"
    );
}

#[test]
fn self_closing_tags_become_open_tags() {
    assert_eq!(normalize("<br />"), "<br>");
    assert_eq!(normalize("<hr/>"), "<hr>");
    assert_eq!(normalize(r#"<img src="a"/>"#), r#"<img src="a">"#);
}

#[test]
fn attributes_are_sorted_and_lowercased() {
    assert_eq!(
        normalize(r#"<a title="bar" HREF="foo">x</a>"#),
        r#"<a href="foo" title="bar">x</a>"#
    );
    assert_eq!(
        normalize(r#"<input type="checkbox" checked="" disabled="">"#),
        r#"<input checked="" disabled="" type="checkbox">"#
    );
}

#[test]
fn valueless_attributes_stay_valueless() {
    assert_eq!(normalize("<input disabled>"), "<input disabled>");
}

#[test]
fn unquoted_and_single_quoted_attribute_values_are_normalised() {
    assert_eq!(normalize("<a href=foo>x</a>"), r#"<a href="foo">x</a>"#);
    assert_eq!(normalize("<a href='foo'>x</a>"), r#"<a href="foo">x</a>"#);
}

#[test]
fn references_become_characters_except_the_four_that_must_stay_escaped() {
    assert_eq!(normalize("&forall;&amp;&gt;&lt;"), "\u{2200}&amp;&gt;&lt;");
    assert_eq!(normalize("&#65;&#x42;"), "AB");
    assert_eq!(normalize("&nbsp;"), "\u{a0}");
}

#[test]
fn unknown_references_are_left_alone() {
    assert_eq!(normalize("&notarealentity;"), "&notarealentity;");
    assert_eq!(normalize("&#xZZ;"), "&#xZZ;");
}

#[test]
fn attribute_values_have_their_references_resolved_then_re_escaped() {
    assert_eq!(
        normalize(r#"<a title="a &amp; b">x</a>"#),
        r#"<a title="a &amp; b">x</a>"#
    );
    assert_eq!(
        normalize(r#"<a title="a &lt; b">x</a>"#),
        r#"<a title="a &lt; b">x</a>"#
    );
}

#[test]
fn comments_and_declarations_survive_verbatim() {
    assert_eq!(normalize("<!-- a > b -->"), "<!-- a > b -->");
    assert_eq!(normalize("<!DOCTYPE html>"), "<!DOCTYPE html>");
    assert_eq!(normalize("<![CDATA[a > b]]>"), "<![CDATA[a > b]]>");
}

#[test]
fn a_newline_after_a_break_is_dropped() {
    assert_eq!(normalize("a<br />\nb"), "a<br>b");
}

#[test]
fn normalisation_is_idempotent() {
    for input in [
        "<p>a  b</p>",
        "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n",
        "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n",
        "<pre><code>keep  me\n</code></pre>",
        "&forall; &amp; <br/>",
    ] {
        let once = normalize(input);
        assert_eq!(normalize(&once), once, "not idempotent for {input:?}");
    }
}

#[test]
fn table_serialisation_differences_normalise_away() {
    let reference = "<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n";
    let compact = "<table><thead><tr><th>a</th></tr></thead></table>";
    assert_eq!(normalize(reference), normalize(compact));
}

#[test]
fn a_real_structural_difference_does_not_normalise_away() {
    // The whole point of the exercise: the normaliser must not be so lenient
    // that a genuine bug slips through.
    assert_ne!(normalize("<p>a</p><p>b</p>"), normalize("<p>a b</p>"));
    assert_ne!(normalize("<em>a</em>"), normalize("<strong>a</strong>"));
    assert_ne!(normalize("<p>a b</p>"), normalize("<p>ab</p>"));
    assert_ne!(
        normalize(r#"<a href="x">t</a>"#),
        normalize(r#"<a href="y">t</a>"#)
    );
    assert_ne!(
        normalize("<pre><code>a  b</code></pre>"),
        normalize("<pre><code>a b</code></pre>")
    );
}

#[test]
fn malformed_input_does_not_panic() {
    for input in [
        "<",
        "<<<",
        "<a href=",
        "<a href=\"unclosed",
        "</>",
        "<!--",
        "&#;",
        "&",
    ] {
        let _ = normalize(input);
    }
}

// ------------------------------------------------------- specification files

#[test]
fn the_commonmark_spec_parses_completely() {
    let examples = parse_spec_json(COMMONMARK_SPEC);
    assert_eq!(examples.len(), 652);
    assert!(examples.iter().all(|e| !e.markdown.is_empty()));
    assert!(examples.iter().all(|e| e.number > 0));
    // A link reference definition renders to nothing, so empty HTML is valid.
    assert!(examples.iter().any(|e| e.html.is_empty()));
    assert!(examples.iter().all(|e| !e.section.is_empty()));
    // Numbering is dense and ordered.
    for (index, example) in examples.iter().enumerate() {
        assert_eq!(example.number, index + 1);
    }
}

#[test]
fn the_gfm_spec_parses_completely() {
    let examples = parse_spec_txt(GFM_SPEC);
    assert_eq!(examples.len(), 672);
    for (index, example) in examples.iter().enumerate() {
        assert_eq!(example.number, index + 1);
    }
    assert!(examples.iter().any(|e| e.section == "Tables (extension)"));
    assert!(examples.iter().any(|e| e.disabled));
}

#[test]
fn spec_txt_tabs_are_decoded() {
    let examples = parse_spec_txt(GFM_SPEC);
    assert!(
        examples.iter().any(|e| e.markdown.contains('\t')),
        "the arrow placeholder was not turned back into a tab"
    );
    assert!(
        !examples.iter().any(|e| e.markdown.contains('\u{2192}')),
        "an arrow placeholder survived"
    );
}

#[test]
fn extension_annotations_are_read_from_the_fence() {
    let examples = parse_spec_txt(GFM_SPEC);
    let tagged: Vec<&str> = examples
        .iter()
        .flat_map(|e| e.extensions.iter().map(String::as_str))
        .collect();
    for name in [
        "autolink",
        "table",
        "strikethrough",
        "tagfilter",
        "disabled",
    ] {
        assert!(tagged.contains(&name), "no example is tagged {name}");
    }
    // A bare `example` fence carries no extensions.
    assert!(examples.iter().any(|e| e.extensions.is_empty()));
}

// ----------------------------------------------------------- comparison mode

#[test]
fn exact_comparison_only_forgives_the_quote_entity() {
    assert_eq!(
        Comparison::Exact.canonicalize("<p>&quot;a&quot;</p>"),
        Comparison::Exact.canonicalize(r#"<p>"a"</p>"#)
    );
    assert_ne!(
        Comparison::Exact.canonicalize("<p>a</p>\n"),
        Comparison::Exact.canonicalize("<p>a</p>")
    );
}

#[test]
fn normalized_comparison_forgives_serialisation_only() {
    assert_eq!(
        Comparison::Normalized.canonicalize("<p>a</p>\n"),
        Comparison::Normalized.canonicalize("<p>a</p>")
    );
    assert_ne!(
        Comparison::Normalized.canonicalize("<p>a</p>"),
        Comparison::Normalized.canonicalize("<p>b</p>")
    );
}
