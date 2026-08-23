//! GFM "extended autolinks" (spec section 6.9).
//!
//! `pulldown-cmark` does not implement this GFM extension, so this module is a
//! faithful port of the reference implementation in `cmark-gfm`
//! (`extensions/autolink.c`), operating on decoded text runs instead of on the
//! raw source chunk.
//!
//! Three link shapes are recognised:
//!
//! * `www.` autolinks   — `http://` is prepended to the href.
//! * URL autolinks      — `http://`, `https://` and `ftp://`.
//! * email autolinks    — `mailto:` is prepended, unless the text already
//!   carries an explicit `mailto:` or `xmpp:` protocol.

/// A single autolink discovered inside a text run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Byte offset of the first character of the link text.
    pub start: usize,
    /// Byte offset one past the last character of the link text.
    pub end: usize,
    /// Destination to place in `href`.
    pub href: String,
}

/// Characters that may directly precede a `www.` autolink.
const WWW_DELIMITERS: [u8; 4] = *b"*_~(";

/// URL schemes recognised by the extended *url* autolink rule.
const SAFE_SCHEMES: [&str; 3] = ["http://", "https://", "ftp://"];

/// `cmark_isspace` — ASCII whitespace only.
fn is_cmark_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Decode the UTF-8 scalar starting at `i`, if any.
fn char_at(data: &[u8], i: usize) -> Option<char> {
    if i >= data.len() {
        return None;
    }
    // Longest possible UTF-8 sequence is 4 bytes.
    let end = (i + 4).min(data.len());
    std::str::from_utf8(&data[i..end])
        .ok()
        .and_then(|s| s.chars().next())
        .or_else(|| {
            // Trailing bytes may split a scalar; retry on shrinking windows.
            (i + 1..end)
                .rev()
                .find_map(|e| std::str::from_utf8(&data[i..e]).ok())
                .and_then(|s| s.chars().next())
        })
}

/// Approximates `cmark_utf8proc_is_punctuation`. ASCII is exact; beyond it we
/// cover the Unicode punctuation blocks that realistically show up in prose.
fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation();
    }
    matches!(
        c,
        '\u{00a1}'
            | '\u{00a7}'
            | '\u{00ab}'
            | '\u{00b6}'
            | '\u{00b7}'
            | '\u{00bb}'
            | '\u{00bf}'
            | '\u{037e}'
            | '\u{0387}'
            | '\u{0589}'
            | '\u{058a}'
            | '\u{05be}'
            | '\u{05c0}'
            | '\u{05c3}'
            | '\u{05c6}'
            | '\u{05f3}'
            | '\u{05f4}'
            | '\u{3030}'
    ) || matches!(u32::from(c),
        0x055a..=0x055f
        | 0x0609..=0x060d
        | 0x061b..=0x061f
        | 0x066a..=0x066d
        | 0x2010..=0x2027
        | 0x2030..=0x205e
        | 0x2e00..=0x2e7f
        | 0x3001..=0x3003
        | 0x3008..=0x3011
        | 0x3014..=0x301f
        | 0xfe10..=0xfe19
        | 0xfe30..=0xfe52
        | 0xfe54..=0xfe61
        | 0xff01..=0xff03
        | 0xff05..=0xff0a
        | 0xff0c..=0xff0f
        | 0xff1a..=0xff20
        | 0xff3b..=0xff3f
        | 0xff5b..=0xff65
    )
}

/// `is_valid_hostchar` — neither Unicode whitespace nor Unicode punctuation.
fn is_valid_hostchar(data: &[u8], i: usize) -> bool {
    match char_at(data, i) {
        Some(c) => !c.is_whitespace() && !is_punctuation(c),
        None => false,
    }
}

/// `autolink_delim` — trims trailing punctuation, unbalanced closing parens and
/// what looks like a trailing entity reference; `<` truncates the link outright.
fn autolink_delim(data: &[u8], mut link_end: usize) -> usize {
    let mut opening = 0usize;
    let mut closing = 0usize;
    let mut cut = link_end;
    for (i, &c) in data.iter().enumerate().take(link_end) {
        match c {
            b'<' => {
                cut = i;
                break;
            }
            b'(' => opening += 1,
            b')' => closing += 1,
            _ => {}
        }
    }
    link_end = cut;

    while link_end > 0 {
        match data[link_end - 1] {
            b')' => {
                if closing <= opening {
                    return link_end;
                }
                closing -= 1;
                link_end -= 1;
            }
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => {
                link_end -= 1;
            }
            b';' => {
                if link_end < 2 {
                    return link_end;
                }
                let mut new_end = link_end - 2;
                while new_end > 0 && data[new_end].is_ascii_alphabetic() {
                    new_end -= 1;
                }
                if new_end < link_end - 2 && data[new_end] == b'&' {
                    link_end = new_end;
                } else {
                    link_end -= 1;
                }
            }
            _ => return link_end,
        }
    }
    link_end
}

/// `check_domain` — returns the length of a valid domain, or 0.
///
/// Underscores are rejected in the last two segments (host names disallow them),
/// except for absurdly long domains where the check is skipped to keep the scan
/// linear (see GHSA-29g3-96g3-jg6c).
fn check_domain(data: &[u8], allow_short: bool) -> usize {
    let size = data.len();
    let (mut np, mut uscore1, mut uscore2) = (0usize, 0usize, 0usize);
    let mut i = 1usize;
    while i + 1 < size {
        if data[i] == b'\\' && i + 2 < size {
            i += 1;
        }
        if data[i] == b'_' {
            uscore2 += 1;
        } else if data[i] == b'.' {
            uscore1 = uscore2;
            uscore2 = 0;
            np += 1;
        } else if !is_valid_hostchar(data, i) && data[i] != b'-' {
            break;
        }
        i += 1;
    }

    if (uscore1 > 0 || uscore2 > 0) && np <= 10 {
        return 0;
    }
    if allow_short || np > 0 { i } else { 0 }
}

/// `sd_autolink_issafe` — does this run start with a scheme we linkify?
fn is_safe_scheme(link: &[u8]) -> bool {
    SAFE_SCHEMES.iter().any(|scheme| {
        let len = scheme.len();
        link.len() > len
            && link[..len].eq_ignore_ascii_case(scheme.as_bytes())
            && is_valid_hostchar(link, len)
    })
}

/// `www_match` — `data` begins at the candidate `w`; `prev` is the character
/// immediately before it in the enclosing inline context.
fn www_match(data: &[u8], prev: Option<u8>) -> Option<usize> {
    if let Some(p) = prev
        && !WWW_DELIMITERS.contains(&p)
        && !is_cmark_space(p)
    {
        return None;
    }
    if data.len() < 4 || !data.starts_with(b"www.") {
        return None;
    }
    let mut link_end = check_domain(data, false);
    if link_end == 0 {
        return None;
    }
    while link_end < data.len() && !is_cmark_space(data[link_end]) && data[link_end] != b'<' {
        link_end += 1;
    }
    let link_end = autolink_delim(data, link_end);
    if link_end == 0 { None } else { Some(link_end) }
}

/// `url_match` — `pos` points at a `:`; rewinds over the scheme that precedes it.
/// Returns the absolute `[start, end)` of the link text.
fn url_match(data: &[u8], pos: usize, floor: usize) -> Option<(usize, usize)> {
    if data.len() - pos < 4 || data[pos + 1] != b'/' || data[pos + 2] != b'/' {
        return None;
    }
    let mut rewind = 0usize;
    while pos - rewind > floor && data[pos - rewind - 1].is_ascii_alphabetic() {
        rewind += 1;
    }
    let start = pos - rewind;
    if !is_safe_scheme(&data[start..]) {
        return None;
    }

    let mut link_end = "://".len();
    let domain_len = check_domain(&data[pos + link_end..], true);
    if domain_len == 0 {
        return None;
    }
    link_end += domain_len;
    while pos + link_end < data.len()
        && !is_cmark_space(data[pos + link_end])
        && data[pos + link_end] != b'<'
    {
        link_end += 1;
    }
    let link_end = autolink_delim(&data[pos..], link_end);
    if link_end == 0 {
        return None;
    }
    Some((start, pos + link_end))
}

/// `validate_protocol` — is `protocol` the text ending at `at - rewind`, and is
/// it itself preceded by a non-alphanumeric?
fn validate_protocol(
    protocol: &str,
    data: &[u8],
    at: usize,
    rewind: usize,
    seg_start: usize,
) -> bool {
    let len = protocol.len();
    let avail = (at - rewind) - seg_start;
    if len > avail {
        return false;
    }
    let from = at - rewind - len;
    if &data[from..from + len] != protocol.as_bytes() {
        return false;
    }
    if len == avail {
        return true;
    }
    !data[from - 1].is_ascii_alphanumeric()
}

/// `postprocess_text` — email / `mailto:` / `xmpp:` autolinks inside `data`,
/// restricted to the byte range `[from, to)`. Offsets in the result are absolute.
fn scan_emails(data: &[u8], from: usize, to: usize, out: &mut Vec<Match>) {
    let mut base = from;
    let mut offset = 0usize;

    'outer: loop {
        if base + offset >= to {
            return;
        }
        let Some(rel) = data[base + offset..to].iter().position(|&c| c == b'@') else {
            return;
        };
        let mut max_rewind = rel;

        // These deliberately survive the `found_at` retry, mirroring the C code.
        let mut auto_mailto = true;
        let mut is_xmpp = false;
        let mut np = 0usize;

        'found_at: loop {
            let at = base + offset + max_rewind;
            let mut rewind = 0usize;
            while rewind < max_rewind {
                let c = data[at - rewind - 1];
                if c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'-' | b'_') {
                    rewind += 1;
                    continue;
                }
                if c == b':' {
                    if validate_protocol("mailto:", data, at, rewind, base + offset) {
                        auto_mailto = false;
                        rewind += 1;
                        continue;
                    }
                    if validate_protocol("xmpp:", data, at, rewind, base + offset) {
                        auto_mailto = false;
                        is_xmpp = true;
                        rewind += 1;
                        continue;
                    }
                }
                break;
            }

            if rewind == 0 {
                offset += max_rewind + 1;
                continue 'outer;
            }

            let mut link_end = 1usize;
            let limit = to - at;
            while link_end < limit {
                let c = data[at + link_end];
                if c.is_ascii_alphanumeric() {
                    link_end += 1;
                    continue;
                }
                if c == b'@' {
                    // Another `@`: retry, treating this one as the separator.
                    offset += max_rewind + 1;
                    max_rewind = link_end - 1;
                    continue 'found_at;
                } else if c == b'.'
                    && link_end + 1 < limit
                    && data[at + link_end + 1].is_ascii_alphanumeric()
                {
                    np += 1;
                } else if c == b'/' && is_xmpp {
                    // xmpp resources may carry a path
                } else if c != b'-' && c != b'_' {
                    break;
                }
                link_end += 1;
            }

            let last = data[at + link_end - 1];
            if link_end < 2 || np == 0 || (!last.is_ascii_alphabetic() && last != b'.') {
                offset += max_rewind + link_end;
                continue 'outer;
            }

            let link_end = autolink_delim(&data[at..to], link_end);
            if link_end == 0 {
                offset += max_rewind + 1;
                continue 'outer;
            }

            let start = at - rewind;
            let end = at + link_end;
            let text = String::from_utf8_lossy(&data[start..end]).into_owned();
            out.push(Match {
                start,
                end,
                href: if auto_mailto {
                    format!("mailto:{text}")
                } else {
                    text
                },
            });

            base = end;
            offset = 0;
            continue 'outer;
        }
    }
}

/// Find every extended autolink in `text`.
///
/// `prev` is the character that immediately precedes `text` in the enclosing
/// inline context (`None` at the start of a block); it gates `www.` autolinks,
/// which may only follow whitespace or one of `*`, `_`, `~`, `(`.
///
/// The returned matches are sorted and non-overlapping.
pub fn scan(text: &str, prev: Option<char>) -> Vec<Match> {
    let data = text.as_bytes();
    let mut out: Vec<Match> = Vec::new();

    // Pass 1: `www.` and URL autolinks, exactly as the inline parser sees them.
    let mut i = 0usize;
    let mut floor = 0usize;
    while i < data.len() {
        match data[i] {
            b'w' => {
                let prev_byte = if i == 0 {
                    prev.filter(char::is_ascii).map(|c| c as u8).or_else(|| {
                        // A non-ASCII predecessor is neither a delimiter nor
                        // ASCII whitespace, so it blocks the match.
                        prev.map(|_| b'x')
                    })
                } else {
                    Some(data[i - 1])
                };
                if let Some(len) = www_match(&data[i..], prev_byte) {
                    let text = &text[i..i + len];
                    out.push(Match {
                        start: i,
                        end: i + len,
                        href: format!("http://{text}"),
                    });
                    i += len;
                    floor = i;
                    continue;
                }
            }
            b':' => {
                if let Some((s, e)) = url_match(data, i, floor) {
                    out.push(Match {
                        start: s,
                        end: e,
                        href: text[s..e].to_string(),
                    });
                    i = e;
                    floor = i;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Pass 2: emails, in the gaps the first pass left behind.
    let mut emails = Vec::new();
    let mut cursor = 0usize;
    for m in &out {
        if cursor < m.start {
            scan_emails(data, cursor, m.start, &mut emails);
        }
        cursor = m.end;
    }
    if cursor < data.len() {
        scan_emails(data, cursor, data.len(), &mut emails);
    }

    out.extend(emails);
    out.sort_by_key(|m| m.start);
    out
}
