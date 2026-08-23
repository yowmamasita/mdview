//! HTML normalisation for specification comparisons.
//!
//! A port of `normalize.py` from the CommonMark / cmark-gfm test suites, which
//! both reference runners apply by default. It folds away differences that no
//! browser can observe — insignificant whitespace, attribute order, self-closing
//! syntax, entity spelling — so a conformance failure means a real difference in
//! the rendered document rather than a difference in serialisation style.

/// Elements around which whitespace carries no meaning.
const BLOCK_TAGS: &[&str] = &[
    "article",
    "header",
    "aside",
    "hgroup",
    "blockquote",
    "hr",
    "iframe",
    "body",
    "li",
    "map",
    "button",
    "object",
    "canvas",
    "ol",
    "caption",
    "output",
    "col",
    "p",
    "colgroup",
    "pre",
    "dd",
    "progress",
    "div",
    "section",
    "dl",
    "table",
    "td",
    "dt",
    "tbody",
    "embed",
    "textarea",
    "fieldset",
    "tfoot",
    "figcaption",
    "th",
    "figure",
    "thead",
    "footer",
    "tr",
    "form",
    "ul",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "video",
    "script",
    "style",
];

/// Named character references that appear in the specification suites.
///
/// Anything outside this table is left verbatim; because both sides of a
/// comparison go through the same function, an unknown entity still compares
/// equal to itself.
const ENTITIES: &[(&str, char)] = &[
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
    ("copy", '©'),
    ("reg", '®'),
    ("trade", '™'),
    ("hellip", '…'),
    ("mdash", '—'),
    ("ndash", '–'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("ldquo", '\u{201c}'),
    ("rdquo", '\u{201d}'),
    ("laquo", '«'),
    ("raquo", '»'),
    ("times", '×'),
    ("divide", '÷'),
    ("deg", '°'),
    ("plusmn", '±'),
    ("frac12", '½'),
    ("frac14", '¼'),
    ("frac34", '¾'),
    ("auml", 'ä'),
    ("ouml", 'ö'),
    ("uuml", 'ü'),
    ("szlig", 'ß'),
    ("eacute", 'é'),
    ("egrave", 'è'),
    ("agrave", 'à'),
    ("ccedil", 'ç'),
    ("ntilde", 'ñ'),
    ("forall", '∀'),
];

/// What the tokenizer produced.
enum Token {
    Text(String),
    Start {
        name: String,
        attrs: Vec<(String, Option<String>)>,
        self_closing: bool,
    },
    End(String),
    /// Comments, doctypes, processing instructions and CDATA, kept verbatim.
    Raw(String),
}

/// Whether `tag` is one of [`BLOCK_TAGS`].
fn is_block(tag: &str) -> bool {
    BLOCK_TAGS.contains(&tag)
}

/// Normalise an HTML fragment.
pub fn normalize(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_pre = false;
    // "starttag", "endtag", "data" or "other" — mirrors normalize.py's `last`.
    let mut last = "starttag";
    let mut last_tag = String::new();

    for token in tokenize(html) {
        match token {
            Token::Text(raw) => {
                let after_tag = last == "endtag" || last == "starttag";
                let after_block_tag = after_tag && is_block(&last_tag);

                let mut data = raw;
                if after_tag && last_tag == "br" {
                    data = data.trim_start_matches('\n').to_string();
                }
                data = decode_refs(&data, in_pre);
                if !in_pre {
                    data = collapse_whitespace(&data);
                }
                if after_block_tag && !in_pre {
                    data = match last {
                        "starttag" => data.trim_start().to_string(),
                        "endtag" => data.trim().to_string(),
                        _ => data,
                    };
                }
                out.push_str(&data);
                last = "data";
            }
            Token::Start {
                name,
                attrs,
                self_closing,
            } => {
                if name == "pre" {
                    in_pre = true;
                }
                if is_block(&name) {
                    truncate_trailing_whitespace(&mut out);
                }
                out.push('<');
                out.push_str(&name);
                let mut attrs = attrs;
                attrs.sort();
                for (key, value) in attrs {
                    out.push(' ');
                    out.push_str(&key);
                    if let Some(value) = value {
                        out.push_str("=\"");
                        out.push_str(&escape_attr(&value));
                        out.push('"');
                    }
                }
                out.push('>');
                last_tag = name;
                last = if self_closing { "endtag" } else { "starttag" };
            }
            Token::End(name) => {
                if name == "pre" {
                    in_pre = false;
                } else if is_block(&name) {
                    truncate_trailing_whitespace(&mut out);
                }
                out.push_str("</");
                out.push_str(&name);
                out.push('>');
                last_tag = name;
                last = "endtag";
            }
            Token::Raw(raw) => {
                out.push_str(&raw);
                last = "other";
            }
        }
    }

    out
}

/// `output.rstrip()` on the buffer built so far.
fn truncate_trailing_whitespace(out: &mut String) {
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
}

/// Collapse every run of whitespace to a single space.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if c.is_ascii_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// Resolve character references, re-escaping the four characters that must stay
/// escaped in a text node.
fn decode_refs(text: &str, _in_pre: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'&' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'&' {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        match parse_reference(&text[i..]) {
            Some((c, len)) => {
                push_char(&mut out, c);
                i += len;
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }

    out
}

/// Parse a single `&…;` reference, returning the character and its length.
fn parse_reference(text: &str) -> Option<(char, usize)> {
    let body = text.strip_prefix('&')?;
    let end = body.find(';')?;
    let name = &body[..end];
    let total = end + 2;

    if let Some(digits) = name.strip_prefix('#') {
        let code = if let Some(hex) = digits.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            digits.parse::<u32>().ok()?
        };
        return char::from_u32(code).map(|c| (c, total));
    }

    ENTITIES
        .iter()
        .find(|(entity, _)| *entity == name)
        .map(|(_, c)| (*c, total))
}

/// `output_char` from normalize.py.
fn push_char(out: &mut String, c: char) {
    match c {
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '&' => out.push_str("&amp;"),
        '"' => out.push_str("&quot;"),
        _ => out.push(c),
    }
}

/// `html.escape(value, quote=True)`.
fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// Split an HTML fragment into tags, text and verbatim markup.
fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            tokens.push(Token::Text(html[start..i].to_string()));
            continue;
        }

        let rest = &html[i..];
        if let Some(len) = verbatim_markup_len(rest) {
            tokens.push(Token::Raw(rest[..len].to_string()));
            i += len;
            continue;
        }

        let Some(close) = rest.find('>') else {
            // An unterminated `<` is literal text.
            tokens.push(Token::Text(rest.to_string()));
            break;
        };
        let inner = &rest[1..close];
        i += close + 1;

        if let Some(name) = inner.strip_prefix('/') {
            tokens.push(Token::End(name.trim().to_ascii_lowercase()));
        } else {
            let (self_closing, inner) = match inner.strip_suffix('/') {
                Some(trimmed) => (true, trimmed),
                None => (false, inner),
            };
            let mut chars = inner.char_indices();
            let name_end = chars
                .find(|(_, c)| c.is_whitespace())
                .map_or(inner.len(), |(idx, _)| idx);
            tokens.push(Token::Start {
                name: inner[..name_end].to_ascii_lowercase(),
                attrs: parse_attributes(&inner[name_end..]),
                self_closing,
            });
        }
    }

    tokens
}

/// Length of a comment, CDATA section, doctype or processing instruction at the
/// start of `text`, if there is one.
fn verbatim_markup_len(text: &str) -> Option<usize> {
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")] {
        if let Some(rest) = text.strip_prefix(open) {
            return Some(match rest.find(close) {
                Some(idx) => open.len() + idx + close.len(),
                None => text.len(),
            });
        }
    }
    if text.starts_with("<!") {
        return Some(text.find('>').map_or(text.len(), |idx| idx + 1));
    }
    None
}

/// Parse the attribute list of a start tag.
fn parse_attributes(text: &str) -> Vec<(String, Option<String>)> {
    let mut attrs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = text[name_start..i].to_ascii_lowercase();
        if name.is_empty() {
            i += 1;
            continue;
        }

        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            attrs.push((name, None));
            continue;
        }
        i += 1; // consume '='
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            attrs.push((name, Some(String::new())));
            break;
        }

        let value = match bytes[i] {
            quote @ (b'"' | b'\'') => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let value = text[start..i].to_string();
                i += 1; // consume closing quote
                value
            }
            _ => {
                let start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                text[start..i].to_string()
            }
        };
        attrs.push((name, Some(decode_attr_refs(&value))));
    }

    attrs
}

/// Attribute values always have their character references resolved.
fn decode_attr_refs(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'&'
            && let Some((c, len)) = parse_reference(&value[i..])
        {
            out.push(c);
            i += len;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i] != b'&' {
            i += 1;
        }
        out.push_str(&value[start..i]);
    }
    out
}
