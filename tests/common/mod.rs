//! Shared plumbing for the specification conformance suites.
#![allow(dead_code)]

pub mod html;

use std::fmt::Write as _;

/// One numbered example lifted from a specification document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Example {
    /// 1-based index within the specification.
    pub number: usize,
    /// Section heading the example appears under.
    pub section: String,
    /// Input Markdown.
    pub markdown: String,
    /// HTML the specification says it must produce.
    pub html: String,
    /// Extensions the example's fence asks for (GFM's `spec.txt` only).
    pub extensions: Vec<String>,
    /// Examples the reference implementation itself skips.
    pub disabled: bool,
}

/// CommonMark ships a machine-readable `spec.json`; parse it.
pub fn parse_spec_json(source: &str) -> Vec<Example> {
    let values: Vec<serde_json::Value> =
        serde_json::from_str(source).expect("spec.json is valid JSON");

    values
        .into_iter()
        .map(|value| Example {
            number: value["example"].as_u64().expect("example number") as usize,
            section: value["section"].as_str().unwrap_or_default().to_string(),
            markdown: value["markdown"].as_str().expect("markdown").to_string(),
            html: value["html"].as_str().expect("html").to_string(),
            extensions: Vec::new(),
            disabled: false,
        })
        .collect()
}

/// GFM only ships `spec.txt`; pull the fenced `example` blocks out of it.
///
/// Each block looks like:
///
/// ```text
/// ```````````````````````````````` example table
/// markdown
/// .
/// html
/// ````````````````````````````````
/// ```
///
/// Words after `example` name the extensions the block exercises — the same
/// annotation cmark-gfm's own runner reads, and what tells a base-CommonMark
/// example apart from one that expects GFM behaviour. Tabs are written as `→`.
pub fn parse_spec_txt(source: &str) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut section = String::new();
    let mut number = 0usize;
    let mut lines = source.lines().peekable();

    while let Some(line) = lines.next() {
        if line == "<!-- END TESTS -->" {
            break;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_string();
            continue;
        }

        let Some((fence_len, extensions)) = example_fence(line) else {
            continue;
        };
        let closing = "`".repeat(fence_len);

        let mut markdown = String::new();
        for body in lines.by_ref() {
            if body == "." {
                break;
            }
            markdown.push_str(body);
            markdown.push('\n');
        }

        let mut html = String::new();
        for body in lines.by_ref() {
            if body == closing {
                break;
            }
            html.push_str(body);
            html.push('\n');
        }

        number += 1;
        examples.push(Example {
            number,
            section: section.clone(),
            markdown: markdown.replace('\u{2192}', "\t"),
            html: html.replace('\u{2192}', "\t"),
            disabled: extensions.iter().any(|e| e == "disabled"),
            extensions,
        });
    }

    examples
}

/// Fence length and requested extensions if `line` opens an example block.
fn example_fence(line: &str) -> Option<(usize, Vec<String>)> {
    let ticks = line.chars().take_while(|&c| c == '`').count();
    if ticks < 3 {
        return None;
    }
    let mut words = line[ticks..].split_whitespace();
    if words.next()? != "example" {
        return None;
    }
    Some((ticks, words.map(str::to_string).collect()))
}

/// How strictly to compare rendered output against the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// Byte-for-byte, apart from `&quot;` (see [`unquote`]).
    Exact,
    /// After [`html::normalize`] — the comparison the reference runners use.
    Normalized,
}

/// Normalise the one difference that is irrelevant even under an exact match.
///
/// The reference implementations write `&quot;` for a double quote in a text
/// node; `pulldown-cmark` leaves it as `"`. HTML5 only requires the escape
/// inside attribute values, where both sides already apply it.
pub fn unquote(html: &str) -> String {
    html.replace("&quot;", "\"")
}

impl Comparison {
    /// Reduce `html` to the form this comparison operates on.
    pub fn canonicalize(self, html: &str) -> String {
        match self {
            Comparison::Exact => unquote(html),
            Comparison::Normalized => unquote(&html::normalize(html)),
        }
    }
}

/// Outcome of running a whole specification suite.
#[derive(Debug, Default)]
pub struct SuiteResult {
    pub total: usize,
    pub passed: usize,
    pub failures: Vec<Failure>,
}

/// A single example that did not match.
#[derive(Debug)]
pub struct Failure {
    pub number: usize,
    pub section: String,
    pub markdown: String,
    pub expected: String,
    pub actual: String,
}

impl SuiteResult {
    /// Example numbers that failed.
    pub fn failed_numbers(&self) -> Vec<usize> {
        self.failures.iter().map(|f| f.number).collect()
    }

    /// A readable report of every failure, capped so output stays usable.
    pub fn report(&self, limit: usize) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{}/{} examples passed ({} failed)",
            self.passed,
            self.total,
            self.failures.len()
        );
        for failure in self.failures.iter().take(limit) {
            let _ = writeln!(
                out,
                "\n--- example {} [{}] ---\nmarkdown:\n{}\nexpected:\n{}\nactual:\n{}",
                failure.number, failure.section, failure.markdown, failure.expected, failure.actual
            );
        }
        if self.failures.len() > limit {
            let _ = writeln!(out, "\n… and {} more", self.failures.len() - limit);
        }
        out
    }
}

/// Run `render` over every example and collect the mismatches.
pub fn run_suite(
    examples: &[Example],
    comparison: Comparison,
    render: impl Fn(&Example) -> String,
) -> SuiteResult {
    let mut result = SuiteResult {
        total: examples.len(),
        ..Default::default()
    };

    for example in examples {
        let actual = render(example);
        if comparison.canonicalize(&actual) == comparison.canonicalize(&example.html) {
            result.passed += 1;
        } else {
            result.failures.push(Failure {
                number: example.number,
                section: example.section.clone(),
                markdown: example.markdown.clone(),
                expected: example.html.clone(),
                actual,
            });
        }
    }

    result
}
