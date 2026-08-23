//! Unit tests for GFM's extended autolink scanner.
//!
//! The specification's own examples are covered by `gfm_spec.rs`; these pin down
//! the rules underneath them, including the ones the spec only states in prose.

use mdview::autolink::{Match, scan};

/// Convenience: the `(text, href)` pairs found in `input`.
fn links(input: &str) -> Vec<(&str, String)> {
    scan(input, None)
        .into_iter()
        .map(|Match { start, end, href }| (&input[start..end], href))
        .collect()
}

/// Convenience: just the linked substrings.
fn texts(input: &str) -> Vec<&str> {
    links(input).into_iter().map(|(text, _)| text).collect()
}

// ---------------------------------------------------------------- www links

#[test]
fn www_gets_an_http_scheme() {
    assert_eq!(
        links("www.commonmark.org"),
        [(
            "www.commonmark.org",
            "http://www.commonmark.org".to_string()
        )]
    );
}

#[test]
fn www_requires_a_dot_separated_domain() {
    assert!(texts("www.").is_empty(), "no domain at all");
    // The period in `www.` itself satisfies the "at least one period" rule, so a
    // single-label host after it is enough — this is what cmark-gfm does.
    assert_eq!(texts("www.example"), ["www.example"]);
    assert_eq!(texts("www.a.b"), ["www.a.b"]);
}

#[test]
fn www_takes_a_trailing_path() {
    assert_eq!(
        texts("Visit www.commonmark.org/help for more."),
        ["www.commonmark.org/help"]
    );
}

#[test]
fn underscores_are_rejected_in_the_last_two_domain_segments() {
    assert_eq!(texts("www.xxx.yyy.zzz"), ["www.xxx.yyy.zzz"]);
    assert!(texts("www.xxx.yyy._zzz").is_empty());
    assert!(texts("www.xxx._yyy.zzz").is_empty());
    assert_eq!(texts("www._xxx.yyy.zzz"), ["www._xxx.yyy.zzz"]);
}

#[test]
fn www_only_starts_after_whitespace_or_a_delimiter() {
    for prefix in ["", " ", "*", "_", "~", "("] {
        let input = format!("{prefix}www.example.com");
        assert_eq!(
            texts(&input).len(),
            1,
            "`{prefix}` should permit a www autolink"
        );
    }
    for prefix in ["x", "1", "-", ".", "/", ":"] {
        let input = format!("{prefix}www.example.com");
        assert!(
            texts(&input).is_empty(),
            "`{prefix}` should block a www autolink"
        );
    }
}

#[test]
fn preceding_character_is_taken_from_the_caller_when_the_run_starts_mid_text() {
    assert_eq!(scan("www.example.com", Some(' ')).len(), 1);
    assert_eq!(scan("www.example.com", Some('(')).len(), 1);
    assert!(scan("www.example.com", Some('`')).is_empty());
    assert!(scan("www.example.com", Some('a')).is_empty());
}

// ---------------------------------------------------------------- url links

#[test]
fn recognised_schemes() {
    assert_eq!(texts("http://commonmark.org"), ["http://commonmark.org"]);
    assert_eq!(texts("https://commonmark.org"), ["https://commonmark.org"]);
    assert_eq!(texts("ftp://foo.bar.baz"), ["ftp://foo.bar.baz"]);
}

#[test]
fn unrecognised_schemes_are_left_alone() {
    assert!(texts("gopher://example.com").is_empty());
    assert!(texts("javascript://example.com").is_empty());
    assert!(texts("file://example.com").is_empty());
}

#[test]
fn a_scheme_glued_to_a_word_is_not_a_link() {
    assert!(texts("xhttp://example.com").is_empty());
}

#[test]
fn scheme_matching_is_case_insensitive() {
    assert_eq!(texts("HTTP://Example.com"), ["HTTP://Example.com"]);
}

// -------------------------------------------------------- path trimming

#[test]
fn trailing_punctuation_is_not_part_of_the_link() {
    assert_eq!(texts("Visit www.commonmark.org."), ["www.commonmark.org"]);
    assert_eq!(
        texts("Visit www.commonmark.org/a.b."),
        ["www.commonmark.org/a.b"]
    );
    for trailer in ['?', '!', '.', ',', ':', '*', '_', '~', '\'', '"'] {
        let input = format!("www.example.com{trailer}");
        assert_eq!(texts(&input), ["www.example.com"], "trailing {trailer:?}");
    }
}

#[test]
fn unbalanced_closing_parentheses_are_trimmed() {
    assert_eq!(
        texts("www.google.com/search?q=Markup+(business)"),
        ["www.google.com/search?q=Markup+(business)"]
    );
    assert_eq!(
        texts("www.google.com/search?q=Markup+(business)))"),
        ["www.google.com/search?q=Markup+(business)"]
    );
    assert_eq!(
        texts("(www.google.com/search?q=Markup+(business))"),
        ["www.google.com/search?q=Markup+(business)"]
    );
}

#[test]
fn interior_parentheses_are_left_alone_when_the_link_does_not_end_in_one() {
    assert_eq!(
        texts("www.google.com/search?q=(business))+ok"),
        ["www.google.com/search?q=(business))+ok"]
    );
}

#[test]
fn a_trailing_entity_reference_is_excluded() {
    assert_eq!(
        texts("www.google.com/search?q=commonmark&hl=en"),
        ["www.google.com/search?q=commonmark&hl=en"]
    );
    assert_eq!(
        texts("www.google.com/search?q=commonmark&hl;"),
        ["www.google.com/search?q=commonmark"]
    );
}

#[test]
fn a_less_than_sign_ends_the_link() {
    assert_eq!(texts("www.commonmark.org/he<lp"), ["www.commonmark.org/he"]);
}

// -------------------------------------------------------------- email links

#[test]
fn plain_email_gets_a_mailto_scheme() {
    assert_eq!(
        links("foo@bar.baz"),
        [("foo@bar.baz", "mailto:foo@bar.baz".to_string())]
    );
}

#[test]
fn plus_is_allowed_before_the_at_but_not_after() {
    assert_eq!(
        texts("hello@mail+xyz.example isn't valid, but hello+xyz@mail.example is."),
        ["hello+xyz@mail.example"]
    );
}

#[test]
fn only_a_dot_may_end_an_email_address() {
    assert_eq!(texts("a.b-c_d@a.b"), ["a.b-c_d@a.b"]);
    assert_eq!(texts("a.b-c_d@a.b."), ["a.b-c_d@a.b"]);
    assert!(texts("a.b-c_d@a.b-").is_empty());
    assert!(texts("a.b-c_d@a.b_").is_empty());
}

#[test]
fn email_needs_a_dot_in_the_domain() {
    assert!(texts("foo@bar").is_empty());
}

#[test]
fn email_scanning_backtracks_from_the_at_sign() {
    // The character before the local part is not in the allowed set, so the
    // scan starts after it rather than abandoning the address.
    assert_eq!(texts("a!b@c.d"), ["b@c.d"]);
}

#[test]
fn explicit_protocols_are_preserved() {
    assert_eq!(
        links("mailto:foo@bar.baz"),
        [("mailto:foo@bar.baz", "mailto:foo@bar.baz".to_string())]
    );
    assert_eq!(
        links("xmpp:foo@bar.baz"),
        [("xmpp:foo@bar.baz", "xmpp:foo@bar.baz".to_string())]
    );
}

#[test]
fn xmpp_addresses_may_carry_a_resource() {
    assert_eq!(
        links("xmpp:foo@bar.baz/txt"),
        [("xmpp:foo@bar.baz/txt", "xmpp:foo@bar.baz/txt".to_string())]
    );
}

// ------------------------------------------------------------------ general

#[test]
fn several_links_in_one_run_are_all_found() {
    let found = texts("see www.a.io and http://b.io or mail c@d.io today");
    assert_eq!(found, ["www.a.io", "http://b.io", "c@d.io"]);
}

#[test]
fn matches_are_sorted_and_never_overlap() {
    let matches = scan("www.a.io x@y.io http://z.io", None);
    assert_eq!(matches.len(), 3);
    for pair in matches.windows(2) {
        assert!(pair[0].end <= pair[1].start, "{pair:?} overlap");
        assert!(pair[0].start < pair[1].start, "{pair:?} out of order");
    }
}

#[test]
fn offsets_are_valid_utf8_boundaries() {
    let input = "héllo — www.exämple.com — ünd";
    for m in scan(input, None) {
        assert!(input.is_char_boundary(m.start));
        assert!(input.is_char_boundary(m.end));
    }
}

#[test]
fn text_without_links_yields_nothing() {
    assert!(scan("just some ordinary prose", None).is_empty());
    assert!(scan("", None).is_empty());
    assert!(scan("@ @ @", None).is_empty());
    assert!(scan("www", None).is_empty());
    assert!(scan("::://", None).is_empty());
}

#[test]
fn pathological_input_terminates() {
    // Guards against the quadratic-scan advisory the reference implementation
    // carries (GHSA-29g3-96g3-jg6c).
    let long = format!("www.{}", "a_.".repeat(4096));
    let _ = scan(&long, None);
    let ats = "@".repeat(8192);
    let _ = scan(&ats, None);
    let mixed = "a@b.".repeat(4096);
    let _ = scan(&mixed, None);
}
