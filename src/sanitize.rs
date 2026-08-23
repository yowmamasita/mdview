//! HTML sanitization.
//!
//! Markdown may embed raw HTML, and a viewer is routinely pointed at files that
//! came from somewhere else. Everything the renderer produces therefore passes
//! through an allowlist before it reaches the webview, so a document cannot run
//! script, pull in remote resources, or smuggle in a form.
//!
//! The allowlist is deliberately a superset of what the renderer emits plus the
//! plain formatting tags people hand-write in Markdown.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use ammonia::Builder;

/// Elements permitted in sanitized output.
const ALLOWED_TAGS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "input",
    "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "picture",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "section",
    "small",
    "source",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
    "var",
    "wbr",
];

/// URL schemes a link or image may point at.
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto", "tel", "xmpp", "file", "data"];

/// Text alignment values `pulldown-cmark` emits on table cells.
const ALLOWED_ALIGNMENTS: &[&str] = &["left", "center", "right"];

/// Elements that may carry an `id` (heading anchors, footnote targets).
const ID_TAGS: &[&str] = &[
    "a", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "section", "span", "sup",
];

/// The shared, immutable sanitizer configuration.
static POLICY: LazyLock<Builder<'static>> = LazyLock::new(build_policy);

/// Sanitize an HTML fragment.
pub fn clean(html: &str) -> String {
    POLICY.clean(html).to_string()
}

/// Whether `class` is one the renderer is allowed to emit on `tag`.
fn class_allowed(tag: &str, class: &str) -> bool {
    match tag {
        "pre" => class == "mermaid",
        "code" => is_language_class(class),
        "input" | "li" | "ul" | "ol" => matches!(class, "task-list-item" | "contains-task-list"),
        "div" | "p" | "blockquote" => {
            class.starts_with("markdown-alert") || class.starts_with("footnote")
        }
        "sup" | "section" | "a" | "span" => class.starts_with("footnote"),
        _ => false,
    }
}

/// `language-rust`, `language-c++`, … as produced for fenced code blocks.
fn is_language_class(class: &str) -> bool {
    class
        .strip_prefix("language-")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(is_language_char))
}

/// Characters permitted in a code fence info string once it becomes a class.
fn is_language_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '#' | '.')
}

/// `text-align: left` and friends, and nothing else.
fn alignment_style(value: &str) -> Option<String> {
    let (property, alignment) = value.trim().trim_end_matches(';').split_once(':')?;
    if property.trim() != "text-align" {
        return None;
    }
    let alignment = alignment.trim();
    ALLOWED_ALIGNMENTS
        .contains(&alignment)
        .then(|| format!("text-align: {alignment}"))
}

/// Build the allowlist policy.
fn build_policy() -> Builder<'static> {
    let mut builder = Builder::default();

    builder
        .tags(HashSet::from_iter(ALLOWED_TAGS.iter().copied()))
        .url_schemes(HashSet::from_iter(ALLOWED_SCHEMES.iter().copied()))
        // Links are opened by the host application, never by the document.
        .link_rel(Some("noopener noreferrer"))
        // Keep relative URLs intact so images next to the document resolve.
        .url_relative(ammonia::UrlRelative::PassThrough);

    let mut attributes: HashMap<&str, HashSet<&str>> = HashMap::new();
    attributes.insert("a", HashSet::from(["href", "title"]));
    attributes.insert(
        "img",
        HashSet::from(["src", "alt", "title", "width", "height"]),
    );
    attributes.insert("source", HashSet::from(["src", "srcset", "type", "media"]));
    attributes.insert("input", HashSet::from(["type", "checked", "disabled"]));
    attributes.insert("ol", HashSet::from(["start", "type"]));
    attributes.insert(
        "th",
        HashSet::from(["style", "colspan", "rowspan", "scope", "align"]),
    );
    attributes.insert(
        "td",
        HashSet::from(["style", "colspan", "rowspan", "align"]),
    );
    attributes.insert("details", HashSet::from(["open"]));
    attributes.insert("del", HashSet::from(["cite"]));
    attributes.insert("ins", HashSet::from(["cite"]));
    attributes.insert("blockquote", HashSet::from(["cite"]));
    for tag in ALLOWED_TAGS {
        attributes.entry(tag).or_default().insert("class");
    }
    for tag in ID_TAGS {
        attributes.entry(tag).or_default().insert("id");
    }
    builder.tag_attributes(attributes);

    // An `<input>` is only ever a task-list checkbox here. Forcing both
    // attributes means a hand-written `<input>` in a document cannot become a
    // text field or an active control, whatever it claims to be.
    builder
        .set_tag_attribute_value("input", "type", "checkbox")
        .set_tag_attribute_value("input", "disabled", "");

    builder.attribute_filter(|tag, attribute, value| match attribute {
        // Only classes the renderer itself produces survive.
        "class" => {
            let kept: Vec<&str> = value
                .split_whitespace()
                .filter(|c| class_allowed(tag, c))
                .collect();
            (!kept.is_empty()).then(|| kept.join(" ").into())
        }
        // Table alignment is the only inline style we understand.
        "style" => alignment_style(value).map(Into::into),
        // Checkboxes exist for task lists and must stay inert.
        "type" if tag == "input" => (value == "checkbox").then(|| value.into()),
        // `data:` is fine for inline images, but not as a link destination.
        "href" if value.trim_start().to_ascii_lowercase().starts_with("data:") => None,
        _ => Some(value.into()),
    });

    builder
}
