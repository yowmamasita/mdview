//! Conformance against the GitHub Flavored Markdown specification.
//!
//! The suite is `spec.txt` from `github/cmark-gfm`, run the way that document
//! annotates itself: an example whose fence names extensions (`example table`,
//! `example autolink`, …) is rendered with the GFM extensions enabled, and an
//! example with a bare `example` fence is rendered as plain CommonMark, because
//! that is what its expected output was written against.
//!
//! Comparison is normalised, matching the reference runner's default.

mod common;

use common::{Comparison, Example};
use mdview::markdown::{RenderOptions, render};

const GFM_SPEC: &str = include_str!("spec/gfm-spec.txt");
const COMMONMARK_SPEC: &str = include_str!("spec/commonmark-0.31.2.json");

/// Examples where our output differs from the reference in a way we accept.
///
/// * **199** — column alignment. `cmark-gfm` writes the obsolete presentational
///   `align="center"` attribute; we write `style="text-align: center"`, which is
///   valid HTML5 and what the bundled stylesheet is built around. The rendered
///   table is identical.
/// * **205** — a header-only table. `cmark-gfm` omits `<tbody>` entirely where
///   `pulldown-cmark` emits an empty one. An empty `<tbody>` contributes no rows
///   and no layout.
const KNOWN_DEVIATIONS: &[usize] = &[199, 205];

/// Examples whose expected output predates CommonMark 0.31.2.
///
/// GFM's `spec.txt` is derived from CommonMark 0.29, and the emphasis rules
/// changed afterwards. Each of these renders exactly as CommonMark 0.31.2 says
/// it should — which `superseded_examples_match_the_current_commonmark_spec`
/// proves rather than assumes.
const SUPERSEDED_BY_COMMONMARK_031: &[usize] = &[398, 426, 434, 435, 436, 473, 474, 475, 477];

fn examples() -> Vec<Example> {
    common::parse_spec_txt(GFM_SPEC)
}

/// Render an example with the options its fence annotation calls for.
fn render_example(example: &Example) -> String {
    let opts = if example.extensions.iter().any(|e| e != "disabled") {
        RenderOptions::gfm()
    } else {
        RenderOptions::commonmark()
    };
    render(&example.markdown, &opts).html
}

#[test]
fn spec_file_parses() {
    let examples = examples();
    assert_eq!(examples.len(), 672, "GFM spec.txt has 672 examples");

    let tagged = |name: &str| {
        examples
            .iter()
            .filter(|e| e.extensions.iter().any(|x| x == name))
            .count()
    };
    assert_eq!(tagged("autolink"), 11);
    assert_eq!(tagged("table"), 8);
    assert_eq!(tagged("strikethrough"), 2);
    assert_eq!(tagged("tagfilter"), 1);
    assert_eq!(tagged("disabled"), 2);
}

#[test]
fn full_specification_passes() {
    let active: Vec<Example> = examples().into_iter().filter(|e| !e.disabled).collect();
    let result = common::run_suite(&active, Comparison::Normalized, render_example);

    let unexpected: Vec<usize> = result
        .failed_numbers()
        .into_iter()
        .filter(|n| !KNOWN_DEVIATIONS.contains(n) && !SUPERSEDED_BY_COMMONMARK_031.contains(n))
        .collect();

    assert!(
        unexpected.is_empty(),
        "GFM conformance regressed on {unexpected:?}:\n{}",
        result.report(10)
    );
}

#[test]
fn conformance_is_at_least_98_percent() {
    let active: Vec<Example> = examples().into_iter().filter(|e| !e.disabled).collect();
    let result = common::run_suite(&active, Comparison::Normalized, render_example);

    assert_eq!(result.total, 670);
    assert!(
        result.passed >= 659,
        "only {}/{} examples pass:\n{}",
        result.passed,
        result.total,
        result.report(5)
    );
}

/// Every extension example — the part of the document that is actually about
/// GFM rather than about CommonMark — must pass.
#[test]
fn every_extension_example_passes() {
    let extension_examples: Vec<Example> = examples()
        .into_iter()
        .filter(|e| !e.disabled && !e.extensions.is_empty())
        .collect();

    assert_eq!(
        extension_examples.len(),
        22,
        "expected 22 extension-tagged examples"
    );

    let result = common::run_suite(&extension_examples, Comparison::Normalized, render_example);
    let unexpected: Vec<usize> = result
        .failed_numbers()
        .into_iter()
        .filter(|n| !KNOWN_DEVIATIONS.contains(n))
        .collect();

    assert!(
        unexpected.is_empty(),
        "GFM extension conformance regressed on {unexpected:?}:\n{}",
        result.report(10)
    );
}

/// The nine emphasis examples that GFM inherited from CommonMark 0.29 are not
/// failures: our output is what CommonMark 0.31.2 requires for the same input.
#[test]
fn superseded_examples_match_the_current_commonmark_spec() {
    let gfm = examples();
    let commonmark = common::parse_spec_json(COMMONMARK_SPEC);
    let opts = RenderOptions::commonmark();

    for number in SUPERSEDED_BY_COMMONMARK_031 {
        let example = gfm
            .iter()
            .find(|e| e.number == *number)
            .unwrap_or_else(|| panic!("GFM example {number} exists"));

        let newer = commonmark
            .iter()
            .find(|c| c.markdown == example.markdown)
            .unwrap_or_else(|| {
                panic!("GFM example {number} has no counterpart in CommonMark 0.31.2")
            });

        let actual = render(&example.markdown, &opts).html;
        assert_eq!(
            Comparison::Normalized.canonicalize(&actual),
            Comparison::Normalized.canonicalize(&newer.html),
            "GFM example {number} (CommonMark 0.31.2 example {}) does not match the current spec",
            newer.number
        );
        assert_ne!(
            Comparison::Normalized.canonicalize(&newer.html),
            Comparison::Normalized.canonicalize(&example.html),
            "GFM example {number} no longer differs from CommonMark 0.31.2; \
             remove it from SUPERSEDED_BY_COMMONMARK_031"
        );
    }
}

/// Guards the deviation list against becoming stale.
#[test]
fn known_deviations_still_deviate() {
    let active: Vec<Example> = examples().into_iter().filter(|e| !e.disabled).collect();
    let result = common::run_suite(&active, Comparison::Normalized, render_example);
    let failed = result.failed_numbers();

    for number in KNOWN_DEVIATIONS {
        assert!(
            failed.contains(number),
            "example {number} now passes; remove it from KNOWN_DEVIATIONS"
        );
    }
}

/// `cmark-gfm` marks the two task-list examples `disabled` because its own
/// output does not match them. Ours does, up to one addition: we tag each item
/// `class="task-list-item"`, which is what GitHub itself serves and what the
/// bundled stylesheet uses to pull the checkbox into the margin. Stripping that
/// one class must leave output identical to the specification.
#[test]
fn disabled_task_list_examples_match_apart_from_the_item_class() {
    let disabled: Vec<Example> = examples().into_iter().filter(|e| e.disabled).collect();
    assert_eq!(disabled.len(), 2);
    assert!(disabled.iter().all(|e| e.section.starts_with("Task list")));

    let opts = RenderOptions::gfm();
    let result = common::run_suite(&disabled, Comparison::Normalized, |e| {
        render(&e.markdown, &opts)
            .html
            .replace("<li class=\"task-list-item\">", "<li>")
    });

    assert!(
        result.failures.is_empty(),
        "task list rendering regressed:\n{}",
        result.report(2)
    );
}

/// The class the previous test strips must actually be there.
#[test]
fn task_list_items_carry_their_class() {
    let html = render("- [x] done\n- [ ] todo\n", &RenderOptions::gfm()).html;
    assert_eq!(
        html.matches("<li class=\"task-list-item\">").count(),
        2,
        "{html}"
    );
}
