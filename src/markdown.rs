//! Markdown → HTML.
//!
//! Built on `pulldown-cmark` (CommonMark 0.31.2 compliant) with the GFM
//! extensions layered on top: tables, task lists, strikethrough, footnotes,
//! alerts, [extended autolinks](crate::autolink) and the `tagfilter` rule for
//! disallowed raw HTML.
//!
//! Fenced code blocks tagged `mermaid` are turned into `<pre class="mermaid">`
//! elements for the viewer's Mermaid runtime to pick up.

use std::collections::HashMap;

use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html,
};

use crate::autolink;

/// Which dialect to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flavor {
    /// Strict CommonMark 0.31.2, no extensions.
    CommonMark,
    /// CommonMark plus the GitHub Flavored Markdown extensions.
    #[default]
    Gfm,
}

/// Tags GFM's `tagfilter` extension neutralises in raw HTML.
const DISALLOWED_TAGS: [&str; 9] = [
    "title",
    "textarea",
    "style",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "script",
    "plaintext",
];

/// Knobs for [`render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Dialect to parse.
    pub flavor: Flavor,
    /// Turn quotes and dashes into typographic equivalents.
    pub smart_punctuation: bool,
    /// Give every heading a GitHub-style slug `id` so `#anchors` resolve.
    pub heading_ids: bool,
    /// Convert ```` ```mermaid ```` fences into `<pre class="mermaid">`.
    pub mermaid: bool,
    /// Swallow a leading `---` YAML front matter block instead of rendering it.
    pub front_matter: bool,
    /// Run the output through the [sanitizer](crate::sanitize).
    pub sanitize: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self::viewer()
    }
}

impl RenderOptions {
    /// Strict CommonMark, no extras — what the CommonMark spec suite expects.
    pub fn commonmark() -> Self {
        Self {
            flavor: Flavor::CommonMark,
            smart_punctuation: false,
            heading_ids: false,
            mermaid: false,
            front_matter: false,
            sanitize: false,
        }
    }

    /// Plain GFM — what the GFM spec suite expects.
    pub fn gfm() -> Self {
        Self {
            flavor: Flavor::Gfm,
            ..Self::commonmark()
        }
    }

    /// What the desktop viewer actually uses: GFM, anchors, Mermaid, front
    /// matter stripped, output sanitized.
    pub fn viewer() -> Self {
        Self {
            flavor: Flavor::Gfm,
            smart_punctuation: false,
            heading_ids: true,
            mermaid: true,
            front_matter: true,
            sanitize: true,
        }
    }
}

/// A heading found while rendering, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1–6.
    pub level: u8,
    /// Slug used as the element's `id`.
    pub id: String,
    /// Plain-text content of the heading.
    pub text: String,
}

/// The result of [`render`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    /// HTML fragment — the document body, without a wrapping element.
    pub html: String,
    /// Text of the first level-1 heading, if the document has one.
    pub title: Option<String>,
    /// Every heading, in document order.
    pub headings: Vec<Heading>,
    /// How many Mermaid diagrams were emitted.
    pub mermaid_blocks: usize,
}

impl Rendered {
    /// Whether the document contains at least one Mermaid diagram.
    pub fn has_mermaid(&self) -> bool {
        self.mermaid_blocks > 0
    }
}

/// Render `input` to an HTML fragment.
pub fn render(input: &str, opts: &RenderOptions) -> Rendered {
    let events: Vec<Event<'_>> = Parser::new_ext(input, parser_options(opts)).collect();

    let (events, mermaid_blocks) = if opts.mermaid {
        extract_mermaid(events)
    } else {
        (events, 0)
    };
    let (events, headings) = if opts.heading_ids {
        apply_heading_ids(events)
    } else {
        (events, Vec::new())
    };
    let events = if opts.flavor == Flavor::Gfm {
        apply_tagfilter(apply_autolinks(apply_gfm_markup(events)))
    } else {
        events
    };

    let mut html_out = String::with_capacity(input.len() * 3 / 2);
    html::push_html(&mut html_out, events.into_iter());
    if opts.sanitize {
        html_out = crate::sanitize::clean(&html_out);
    }

    let title = headings
        .iter()
        .find(|h| h.level == 1)
        .map(|h| h.text.clone());

    Rendered {
        html: html_out,
        title,
        headings,
        mermaid_blocks,
    }
}

/// Translate [`RenderOptions`] into `pulldown-cmark` parser options.
fn parser_options(opts: &RenderOptions) -> Options {
    let mut o = Options::empty();
    if opts.flavor == Flavor::Gfm {
        o.insert(Options::ENABLE_TABLES);
        o.insert(Options::ENABLE_STRIKETHROUGH);
        o.insert(Options::ENABLE_TASKLISTS);
        o.insert(Options::ENABLE_FOOTNOTES);
        o.insert(Options::ENABLE_GFM);
    }
    if opts.smart_punctuation {
        o.insert(Options::ENABLE_SMART_PUNCTUATION);
    }
    if opts.front_matter {
        o.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    }
    o
}

/// Escape text for placement in an HTML text node or double-quoted attribute.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Replace ```` ```mermaid ```` fences with `<pre class="mermaid">` blocks.
fn extract_mermaid(events: Vec<Event<'_>>) -> (Vec<Event<'_>>, usize) {
    let mut out = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();
    let mut count = 0usize;

    while let Some(event) = iter.next() {
        let is_mermaid = matches!(
            &event,
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if info.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("mermaid"))
        );
        if !is_mermaid {
            out.push(event);
            continue;
        }

        let mut source = String::new();
        for inner in iter.by_ref() {
            match inner {
                Event::End(TagEnd::CodeBlock) => break,
                Event::Text(t) => source.push_str(&t),
                Event::Code(t) => source.push_str(&t),
                Event::Html(t) | Event::InlineHtml(t) => source.push_str(&t),
                Event::SoftBreak | Event::HardBreak => source.push('\n'),
                _ => {}
            }
        }
        count += 1;
        out.push(Event::Html(CowStr::from(format!(
            "<pre class=\"mermaid\">{}</pre>\n",
            escape_html(source.trim_end_matches('\n'))
        ))));
    }

    (out, count)
}

/// GitHub-style heading slug: lowercase, punctuation dropped, spaces hyphenated.
///
/// Surrounding whitespace is trimmed first, so `# Title ` and `# Title` agree.
pub fn slugify(text: &str) -> String {
    let text = text.trim();
    let mut slug = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            slug.extend(c.to_lowercase());
        } else if c.is_whitespace() {
            slug.push('-');
        }
    }
    slug
}

/// Attach slug `id`s to headings that do not already declare one, and collect
/// the resulting table of contents.
fn apply_heading_ids(mut events: Vec<Event<'_>>) -> (Vec<Event<'_>>, Vec<Heading>) {
    let mut headings = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    let starts: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Event::Start(Tag::Heading { .. })))
        .map(|(i, _)| i)
        .collect();

    for i in starts {
        let mut text = String::new();
        for event in &events[i + 1..] {
            match event {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Text(t) | Event::Code(t) => text.push_str(t),
                Event::SoftBreak | Event::HardBreak => text.push(' '),
                _ => {}
            }
        }

        let Event::Start(Tag::Heading { level, id, .. }) = &mut events[i] else {
            continue;
        };
        let level_num = heading_level_number(*level);

        let slug = match id {
            Some(existing) => existing.to_string(),
            None => {
                let base = slugify(text.trim());
                let base = if base.is_empty() {
                    "section".to_string()
                } else {
                    base
                };
                let n = seen.entry(base.clone()).or_insert(0);
                let unique = if *n == 0 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                };
                *n += 1;
                *id = Some(CowStr::from(unique.clone()));
                unique
            }
        };

        headings.push(Heading {
            level: level_num,
            id: slug,
            text: text.trim().to_string(),
        });
    }

    (events, headings)
}

/// `HeadingLevel` as a plain number.
fn heading_level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Give GFM's alerts and task list items the markup GitHub produces.
///
/// `pulldown-cmark` recognises both constructs but renders them minimally: an
/// alert becomes a bare `<blockquote class="markdown-alert-note">` with its
/// title line dropped, and a task list item gets a checkbox but no class to
/// hang layout on. Both are rewritten here so the stylesheet has something to
/// target and the result matches what a reader expects from GitHub.
fn apply_gfm_markup(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut out = Vec::with_capacity(events.len());
    let mut iter = events.into_iter().peekable();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let (class, title) = alert_kind(kind);
                out.push(Event::Html(CowStr::from(format!(
                    "<div class=\"markdown-alert markdown-alert-{class}\">\n\
                     <p class=\"markdown-alert-title\">{title}</p>\n"
                ))));
            }
            Event::End(TagEnd::BlockQuote(Some(_))) => {
                out.push(Event::Html(CowStr::Borrowed("</div>\n")));
            }
            // A list item whose first event is a task marker is a task list item.
            Event::Start(Tag::Item) if matches!(iter.peek(), Some(Event::TaskListMarker(_))) => {
                let Some(Event::TaskListMarker(checked)) = iter.next() else {
                    unreachable!("peeked a task list marker")
                };
                let checked = if checked { " checked=\"\"" } else { "" };
                out.push(Event::Html(CowStr::from(format!(
                    "<li class=\"task-list-item\">\
                     <input{checked} disabled=\"\" type=\"checkbox\"> "
                ))));
            }
            other => out.push(other),
        }
    }

    out
}

/// CSS suffix and heading text for an alert kind.
fn alert_kind(kind: BlockQuoteKind) -> (&'static str, &'static str) {
    match kind {
        BlockQuoteKind::Note => ("note", "Note"),
        BlockQuoteKind::Tip => ("tip", "Tip"),
        BlockQuoteKind::Important => ("important", "Important"),
        BlockQuoteKind::Warning => ("warning", "Warning"),
        BlockQuoteKind::Caution => ("caution", "Caution"),
    }
}

/// Turn bare URLs, `www.` hosts and email addresses in text runs into links,
/// per the GFM autolink extension. Text inside an existing link is left alone.
fn apply_autolinks(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    // `cmark-gfm` consolidates adjacent text nodes before looking for
    // autolinks, and so must we: `pulldown-cmark` splits a run of text at every
    // character that *might* have opened an inline, so `a@b.c_` arrives as two
    // events and would otherwise be scanned as if the trailing `_` were absent.
    let events = coalesce_text(events);
    let mut out = Vec::with_capacity(events.len());
    let mut link_depth = 0usize;
    // Code blocks and metadata blocks hold literal text, never autolinks.
    let mut literal_depth = 0usize;
    // The character preceding the current run, which gates `www.` autolinks.
    let mut prev: Option<char> = None;

    for event in events {
        // Block boundaries reset the "preceding character" context.
        if matches!(event, Event::Start(_) | Event::End(_)) {
            prev = closing_delimiter(&event);
        }

        match &event {
            Event::Start(Tag::Link { .. }) | Event::Start(Tag::Image { .. }) => link_depth += 1,
            Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => {
                link_depth = link_depth.saturating_sub(1)
            }
            Event::Start(Tag::CodeBlock(_)) | Event::Start(Tag::MetadataBlock(_)) => {
                literal_depth += 1
            }
            Event::End(TagEnd::CodeBlock) | Event::End(TagEnd::MetadataBlock(_)) => {
                literal_depth = literal_depth.saturating_sub(1)
            }
            _ => {}
        }

        match event {
            Event::Text(text) if link_depth == 0 && literal_depth == 0 => {
                let matches = autolink::scan(&text, prev);
                prev = text.chars().last().or(prev);
                if matches.is_empty() {
                    out.push(Event::Text(text));
                    continue;
                }
                let mut cursor = 0usize;
                for m in matches {
                    if cursor < m.start {
                        out.push(Event::Text(CowStr::from(text[cursor..m.start].to_string())));
                    }
                    out.push(Event::Start(Tag::Link {
                        link_type: pulldown_cmark::LinkType::Autolink,
                        dest_url: CowStr::from(m.href),
                        title: CowStr::from(""),
                        id: CowStr::from(""),
                    }));
                    out.push(Event::Text(CowStr::from(text[m.start..m.end].to_string())));
                    out.push(Event::End(TagEnd::Link));
                    cursor = m.end;
                }
                if cursor < text.len() {
                    out.push(Event::Text(CowStr::from(text[cursor..].to_string())));
                }
            }
            other => {
                if let Event::Text(t) = &other {
                    prev = t.chars().last().or(prev);
                } else if let Some(c) = trailing_char(&other) {
                    prev = Some(c);
                }
                out.push(other);
            }
        }
    }

    out
}

/// Merge runs of adjacent [`Event::Text`] into single events.
fn coalesce_text(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut out = Vec::with_capacity(events.len());
    let mut run: Vec<CowStr<'_>> = Vec::new();

    for event in events {
        match event {
            Event::Text(text) => run.push(text),
            other => {
                flush_text_run(&mut run, &mut out);
                out.push(other);
            }
        }
    }
    flush_text_run(&mut run, &mut out);

    out
}

/// Emit a pending run of text as one event, borrowing when it is already whole.
fn flush_text_run<'a>(run: &mut Vec<CowStr<'a>>, out: &mut Vec<Event<'a>>) {
    match run.len() {
        0 => {}
        1 => out.push(Event::Text(run.pop().expect("run holds one item"))),
        _ => {
            let mut merged = String::with_capacity(run.iter().map(|t| t.len()).sum());
            for part in run.drain(..) {
                merged.push_str(&part);
            }
            out.push(Event::Text(CowStr::from(merged)));
        }
    }
    run.clear();
}

/// The character that would close this inline construct in the source, used to
/// decide whether a following `www.` run is at a valid autolink boundary.
fn closing_delimiter(event: &Event<'_>) -> Option<char> {
    match event {
        Event::End(TagEnd::Emphasis) => Some('*'),
        Event::End(TagEnd::Strong) => Some('*'),
        Event::End(TagEnd::Strikethrough) => Some('~'),
        Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => Some(')'),
        Event::End(TagEnd::FootnoteDefinition) => Some(']'),
        // Entering or leaving a block starts a fresh line.
        _ => None,
    }
}

/// The character that trails a non-text inline event.
fn trailing_char(event: &Event<'_>) -> Option<char> {
    match event {
        Event::Code(_) => Some('`'),
        Event::Html(_) | Event::InlineHtml(_) => Some('>'),
        Event::FootnoteReference(_) => Some(']'),
        Event::SoftBreak | Event::HardBreak => Some('\n'),
        Event::Rule => Some('\n'),
        _ => None,
    }
}

/// GFM `tagfilter`: neutralise raw `<script>`, `<style>`, `<iframe>` &co by
/// escaping their leading `<`.
fn apply_tagfilter(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    events
        .into_iter()
        .map(|event| match event {
            Event::Html(t) => match filter_tags(&t) {
                Some(filtered) => Event::Html(CowStr::from(filtered)),
                None => Event::Html(t),
            },
            Event::InlineHtml(t) => match filter_tags(&t) {
                Some(filtered) => Event::InlineHtml(CowStr::from(filtered)),
                None => Event::InlineHtml(t),
            },
            other => other,
        })
        .collect()
}

/// Returns the filtered string, or `None` when nothing needed escaping.
fn filter_tags(html: &str) -> Option<String> {
    let bytes = html.as_bytes();
    let mut hits = Vec::new();
    for (i, &c) in bytes.iter().enumerate() {
        if c == b'<' && DISALLOWED_TAGS.iter().any(|t| is_tag(&bytes[i..], t)) {
            hits.push(i);
        }
    }
    if hits.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(html.len() + hits.len() * 3);
    let mut cursor = 0usize;
    for i in hits {
        out.push_str(&html[cursor..i]);
        out.push_str("&lt;");
        cursor = i + 1;
    }
    out.push_str(&html[cursor..]);
    Some(out)
}

/// `is_tag` from cmark-gfm's tagfilter: does `data` open or close `tagname`?
fn is_tag(data: &[u8], tagname: &str) -> bool {
    if data.len() < 3 || data[0] != b'<' {
        return false;
    }
    let mut i = 1usize;
    if data[i] == b'/' {
        i += 1;
    }
    for &expected in tagname.as_bytes() {
        if i >= data.len() || data[i].to_ascii_lowercase() != expected {
            return false;
        }
        i += 1;
    }
    if i >= data.len() {
        return false;
    }
    if data[i].is_ascii_whitespace() || data[i] == b'>' {
        return true;
    }
    data[i] == b'/' && data.len() >= i + 2 && data[i + 1] == b'>'
}
