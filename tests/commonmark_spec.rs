//! Conformance against the CommonMark 0.31.2 specification.
//!
//! The suite is the official `spec.json`, run verbatim: all 652 examples, with
//! their expected HTML compared byte for byte (modulo the `&quot;` normalisation
//! documented on [`common::normalize`]).

mod common;

use common::Comparison;
use mdview::markdown::{RenderOptions, render};

const SPEC: &str = include_str!("spec/commonmark-0.31.2.json");

/// The single example where output differs structurally from the reference
/// implementation.
///
/// Example 175 is an HTML block opening a list item:
///
/// ```text
/// - <div>
/// - foo
/// ```
///
/// `cmark` writes `<li>\n<div>` where `pulldown-cmark` writes `<li><div>`. The
/// rendered result is identical; only the source whitespace differs. Listing it
/// here rather than loosening the comparison keeps every other example strict.
const KNOWN_DEVIATIONS: &[usize] = &[175];

fn examples() -> Vec<common::Example> {
    common::parse_spec_json(SPEC)
}

#[test]
fn spec_file_parses() {
    let examples = examples();
    assert_eq!(examples.len(), 652, "CommonMark 0.31.2 has 652 examples");
    assert_eq!(examples[0].section, "Tabs");
    assert_eq!(examples[0].markdown, "\tfoo\tbaz\t\tbim\n");
    assert_eq!(examples.last().unwrap().section, "Textual content");
}

#[test]
fn full_specification_passes() {
    let opts = RenderOptions::commonmark();
    let result = common::run_suite(&examples(), Comparison::Exact, |ex| {
        render(&ex.markdown, &opts).html
    });

    let unexpected: Vec<usize> = result
        .failed_numbers()
        .into_iter()
        .filter(|n| !KNOWN_DEVIATIONS.contains(n))
        .collect();

    assert!(
        unexpected.is_empty(),
        "CommonMark conformance regressed on {unexpected:?}:\n{}",
        result.report(10)
    );
}

/// Guards the deviation list: if `pulldown-cmark` ever fixes example 175 we want
/// to hear about it rather than silently keep an obsolete exception.
#[test]
fn known_deviations_still_deviate() {
    let opts = RenderOptions::commonmark();
    let result = common::run_suite(&examples(), Comparison::Exact, |ex| {
        render(&ex.markdown, &opts).html
    });
    let failed = result.failed_numbers();

    for number in KNOWN_DEVIATIONS {
        assert!(
            failed.contains(number),
            "example {number} now passes; remove it from KNOWN_DEVIATIONS"
        );
    }
}

#[test]
fn conformance_is_at_least_999_per_mille() {
    let opts = RenderOptions::commonmark();
    let result = common::run_suite(&examples(), Comparison::Exact, |ex| {
        render(&ex.markdown, &opts).html
    });

    assert_eq!(result.total, 652);
    assert!(
        result.passed >= 651,
        "only {}/{} examples pass:\n{}",
        result.passed,
        result.total,
        result.report(5)
    );
}

/// The GFM configuration is a superset of CommonMark, so it must not disturb
/// plain CommonMark documents except where GFM deliberately changes the rules:
/// raw HTML filtering and autolinking bare URLs.
#[test]
fn gfm_configuration_stays_commonmark_compatible() {
    let opts = RenderOptions::gfm();
    let result = common::run_suite(&examples(), Comparison::Exact, |ex| {
        render(&ex.markdown, &opts).html
    });

    let unexpected: Vec<usize> = result
        .failed_numbers()
        .into_iter()
        .filter(|n| !KNOWN_DEVIATIONS.contains(n))
        .collect();

    // Every remaining difference must be explained by one of the two GFM
    // extensions that intentionally change CommonMark output.
    let explained: Vec<usize> = unexpected
        .iter()
        .copied()
        .filter(|n| {
            let example = examples().into_iter().find(|e| e.number == *n).unwrap();
            let commonmark = render(&example.markdown, &RenderOptions::commonmark()).html;
            let gfm = render(&example.markdown, &opts).html;
            differs_only_by_gfm_extensions(&commonmark, &gfm)
        })
        .collect();

    assert_eq!(
        unexpected,
        explained,
        "GFM mode changed CommonMark output for reasons other than tagfilter or autolinks:\n{}",
        result.report(10)
    );
}

/// True when the only changes are escaped disallowed tags or added autolinks.
fn differs_only_by_gfm_extensions(commonmark: &str, gfm: &str) -> bool {
    let mut undone = gfm.to_string();
    for tag in [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ] {
        undone = undone.replace(&format!("&lt;{tag}"), &format!("<{tag}"));
        undone = undone.replace(&format!("&lt;/{tag}"), &format!("</{tag}"));
        undone = undone.replace(
            &format!("&lt;{}", tag.to_uppercase()),
            &format!("<{}", tag.to_uppercase()),
        );
    }
    undone == commonmark || strip_tags(&undone) == strip_tags(commonmark)
}

/// Crude tag stripper, enough to compare text content after autolinking.
fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in html.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}
