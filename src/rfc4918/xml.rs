//! # XML scanner
//!
//! The tokenizer under the [`crate::rfc4918`] multistatus parsers, for the
//! XML subset WebDAV servers send: elements, attributes, text, CDATA,
//! comments and processing instructions. A DOCTYPE is skipped, internal
//! subset included, its entities left unexpanded like any unknown reference.
//! Malformed markup ends the scan short of the body's end, which a parser
//! reads as an incomplete document.
//!
//! Names, indentation and short values are searched 16 bytes at a time with
//! the vector compares of [`lanes`], longer text with memchr's kernels. A
//! text event says how much of it needs no decoding, found by the same search
//! that finds its end, and the decoder searches the same way for the bytes it
//! rewrites, copying the runs between them whole.

mod lanes;

use alloc::string::String;

use memchr::{memchr, memchr2, memchr3, memmem};

use crate::rfc4918::xml::lanes::Lanes;

/// One markup event, borrowed from the body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum XmlEvent<'a> {
    /// A start tag, `empty` for `<a/>`; `prefix` is empty for an unprefixed
    /// name, `attrs` is the raw attribute area.
    Start {
        prefix: &'a str,
        local: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    /// An end tag.
    End(&'a str),
    /// Raw character data, its first `clean` bytes holding neither `&` nor
    /// CR.
    Text { raw: &'a str, clean: usize },
    /// The content of a CDATA section.
    Cdata(&'a str),
}

/// Iterator of [`XmlEvent`]s over a body.
pub(crate) struct XmlScanner<'a> {
    xml: &'a str,
    pos: usize,
}

impl<'a> XmlScanner<'a> {
    pub(crate) fn new(xml: &'a str) -> Self {
        Self { xml, pos: 0 }
    }

    /// Whether the scan reached the end of the body.
    pub(crate) fn is_done(&self) -> bool {
        self.pos == self.xml.len()
    }
}

impl<'a> Iterator for XmlScanner<'a> {
    type Item = XmlEvent<'a>;

    #[inline(always)]
    fn next(&mut self) -> Option<XmlEvent<'a>> {
        loop {
            let bytes = self.xml.as_bytes();
            let at = self.pos;
            let first = *bytes.get(at)?;

            if first != b'<' {
                let (end, clean) = match text_stop(bytes, at) {
                    Some(i) if bytes[i] == b'<' => (i, i - at),
                    Some(i) => (
                        memchr(b'<', &bytes[i..]).map_or(bytes.len(), |k| i + k),
                        i - at,
                    ),
                    None => (bytes.len(), bytes.len() - at),
                };

                self.pos = end;
                let raw = &self.xml[at..end];
                return Some(XmlEvent::Text { raw, clean });
            }

            match bytes.get(at + 1)? {
                b'/' => {
                    let (end, colon) = name_end::<false>(bytes, at + 2)?;
                    self.pos = end + 1;

                    let mut local = &self.xml[colon.map_or(at + 2, |c| c + 1)..end];
                    if !bytes[end - 1].is_ascii_alphanumeric() {
                        local = local.trim_end();
                    }
                    return Some(XmlEvent::End(local));
                }
                b'?' => {
                    let end = memmem::find(&bytes[at + 2..], b"?>")?;
                    self.pos = at + 2 + end + 2;
                }
                b'!' if bytes[at..].starts_with(b"<!--") => {
                    let end = memmem::find(&bytes[at + 4..], b"-->")?;
                    self.pos = at + 4 + end + 3;
                }
                b'!' if bytes[at..].starts_with(b"<![CDATA[") => {
                    let end = memmem::find(&bytes[at + 9..], b"]]>")?;
                    self.pos = at + 9 + end + 3;
                    return Some(XmlEvent::Cdata(&self.xml[at + 9..at + 9 + end]));
                }
                b'!' => {
                    let mut subset = false;
                    let mut i = at + 2;
                    let end = loop {
                        match *bytes.get(i)? {
                            quote @ (b'"' | b'\'') => i += 1 + memchr(quote, &bytes[i + 1..])?,
                            b'[' => subset = true,
                            b']' => subset = false,
                            b'>' if !subset => break i,
                            _ => {}
                        }
                        i += 1;
                    };
                    self.pos = end + 1;
                }
                _ => {
                    let (name_end, colon) = name_end::<true>(bytes, at + 1)?;
                    if name_end == at + 1 {
                        return None;
                    }

                    let end = match bytes[name_end] {
                        b'>' => name_end,
                        b'/' if bytes.get(name_end + 1) == Some(&b'>') => name_end + 1,
                        _ => {
                            let mut i = name_end;
                            loop {
                                let j = i + memchr3(b'>', b'"', b'\'', &bytes[i..])?;
                                match bytes[j] {
                                    b'>' => break j,
                                    quote => i = j + 2 + memchr(quote, &bytes[j + 1..])?,
                                }
                            }
                        }
                    };

                    let empty = end > name_end && bytes[end - 1] == b'/';
                    self.pos = end + 1;
                    return Some(XmlEvent::Start {
                        prefix: colon.map_or("", |c| &self.xml[at + 1..c]),
                        local: &self.xml[colon.map_or(at + 1, |c| c + 1)..name_end],
                        attrs: &self.xml[name_end..end - empty as usize],
                        empty,
                    });
                }
            }
        }
    }
}

/// The first `<`, `&` or CR from `from`: probed 16 bytes at a time over the
/// short spans markup holds, then searched by memchr.
#[inline(always)]
fn text_stop(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;

    for _ in 0..4 {
        let Some(block) = bytes[i..].first_chunk() else {
            break;
        };

        let lanes = Lanes::new(block);
        let stops = lanes.eq(b'<') | lanes.eq(b'&') | lanes.eq(b'\r');
        if let Some(lane) = stops.mask().first() {
            return Some(i + lane);
        }

        i += 16;
    }

    memchr3(b'<', b'&', b'\r', &bytes[i..]).map(|k| i + k)
}

/// The first byte from `from` that ends a name, and the first colon before
/// it: a blank, `/` or `>` in a start tag (`OPEN`), `>` in an end tag.
#[inline(always)]
fn name_end<const OPEN: bool>(bytes: &[u8], from: usize) -> Option<(usize, Option<usize>)> {
    let mut i = from;
    let mut colon = None;
    let mut tail = [0; 16];

    // NOTE: zeros pad the last block, ending no name.
    while i < bytes.len() {
        let block = match bytes[i..].first_chunk() {
            Some(block) => block,
            None => {
                tail[..bytes.len() - i].copy_from_slice(&bytes[i..]);
                &tail
            }
        };

        let lanes = Lanes::new(block);
        let ends = match OPEN {
            true => {
                lanes.eq(b' ')
                    | lanes.eq(b'\t')
                    | lanes.eq(b'\n')
                    | lanes.eq(b'\r')
                    | lanes.eq(b'/')
                    | lanes.eq(b'>')
            }
            false => lanes.eq(b'>'),
        };
        let end = ends.mask();

        if colon.is_none() {
            colon = lanes.eq(b':').mask().before(end).first().map(|c| i + c);
        }

        if let Some(lane) = end.first() {
            return Some((i + lane, colon));
        }

        i += 16;
    }

    None
}

/// Whether two names are equal, compared inline: names are short, and a
/// slice comparison is a call.
#[inline(always)]
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let n = a.len();
    if n != b.len() {
        return false;
    }

    match n {
        0 => true,
        1..4 => a[0] == b[0] && a[n / 2] == b[n / 2] && a[n - 1] == b[n - 1],
        4..8 => {
            a.first_chunk::<4>() == b.first_chunk::<4>()
                && a.last_chunk::<4>() == b.last_chunk::<4>()
        }
        8..=16 => {
            a.first_chunk::<8>() == b.first_chunk::<8>()
                && a.last_chunk::<8>() == b.last_chunk::<8>()
        }
        _ => a == b,
    }
}

/// Iterator of `(qualified name, raw value)` over a raw attribute area,
/// namespace declarations included.
#[derive(Clone, Copy, Debug)]
pub(crate) struct XmlAttrs<'a>(pub(crate) &'a str);

impl<'a> Iterator for XmlAttrs<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }

        let rest = self.0.trim_start_matches([' ', '\t', '\n', '\r']);
        self.0 = "";

        let eq = rest.find('=')?;
        let name = rest[..eq].trim_end();
        let value = rest[eq + 1..].trim_start_matches([' ', '\t', '\n', '\r']);
        let quote = match value.as_bytes().first()? {
            b'"' => '"',
            b'\'' => '\'',
            _ => return None,
        };
        let end = value[1..].find(quote)?;

        self.0 = &value[end + 2..];
        Some((name, &value[1..1 + end]))
    }
}

/// Returns the raw value of the unprefixed attribute `want`, an empty prefix
/// (`:name`) counting as none, the last one when repeated.
pub(crate) fn attr<'a>(area: &'a str, want: &str) -> Option<&'a str> {
    XmlAttrs(area)
        .filter(|(name, _)| match name.split_once(':') {
            Some((prefix, local)) => prefix.is_empty() && same_name(local, want),
            None => same_name(name, want),
        })
        .last()
        .map(|(_, value)| value)
}

/// Appends decoded character data, its first `clean` bytes as they are: line
/// ends normalised to LF (XML 1.0 §2.11) and references resolved, in one
/// pass.
pub(crate) fn decode_text(raw: &str, clean: usize, out: &mut String) {
    let bytes = raw.as_bytes();
    let mut i = clean;

    out.reserve(raw.len());
    out.push_str(&raw[..clean]);

    while i < bytes.len() {
        let k = special(bytes, i);
        out.push_str(&raw[i..k]);

        let Some(&byte) = bytes.get(k) else {
            break;
        };

        if byte == b'\r' {
            out.push('\n');
            i = k + 1 + (bytes.get(k + 1) == Some(&b'\n')) as usize;
            continue;
        }

        let (ch, len) = match bytes[k + 1..] {
            [b'#', b'1', b'3', b';', ..] => ('\r', 5),
            [b'a', b'm', b'p', b';', ..] => ('&', 5),
            [b'l', b't', b';', ..] => ('<', 4),
            [b'g', b't', b';', ..] => ('>', 4),
            [b'q', b'u', b'o', b't', b';', ..] => ('"', 6),
            [b'a', b'p', b'o', b's', b';', ..] => ('\'', 6),
            _ => ('\0', 0),
        };

        if len > 0 {
            out.push(ch);
            i = k + len;
            continue;
        }

        let Some(end) = memchr(b';', &bytes[k..]) else {
            push_lines(&raw[k..], out);
            return;
        };

        let end = k + end;
        match resolve_reference(&raw[k + 1..end]) {
            Some(ch) => out.push(ch),
            None => push_lines(&raw[k..=end], out),
        }
        i = end + 1;
    }
}

/// The first `&` or CR from `from`, or the end: probed 16 bytes at a time
/// over the short lines text holds, then searched by memchr.
#[inline(always)]
fn special(bytes: &[u8], from: usize) -> usize {
    let mut i = from;

    for _ in 0..4 {
        let Some(block) = bytes[i..].first_chunk() else {
            break;
        };

        let lanes = Lanes::new(block);
        if let Some(lane) = (lanes.eq(b'&') | lanes.eq(b'\r')).mask().first() {
            return i + lane;
        }

        i += 16;
    }

    memchr2(b'&', b'\r', &bytes[i..]).map_or(bytes.len(), |k| i + k)
}

/// Decodes a raw attribute value: each literal whitespace becomes a space
/// (XML 1.0 §3.3.3) and references resolve.
pub(crate) fn decode_attr(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(' ');
                i += 1 + (bytes.get(i + 1) == Some(&b'\n')) as usize;
            }
            b'\t' | b'\n' => {
                out.push(' ');
                i += 1;
            }
            b'&' => {
                let Some(end) = raw[i..].find(';') else {
                    push_spaces(&raw[i..], &mut out);
                    break;
                };

                let end = i + end;
                match resolve_reference(&raw[i + 1..end]) {
                    Some(ch) => out.push(ch),
                    None => push_spaces(&raw[i..=end], &mut out),
                }
                i = end + 1;
            }
            _ => {
                let next = raw[i..]
                    .find(['\r', '\t', '\n', '&'])
                    .map_or(raw.len(), |n| i + n);
                out.push_str(&raw[i..next]);
                i = next;
            }
        }
    }

    out
}

/// Pushes `raw` with its line ends normalised, references left alone.
fn push_lines(raw: &str, out: &mut String) {
    let mut rest = raw;
    while let Some(k) = rest.find('\r') {
        out.push_str(&rest[..k]);
        out.push('\n');
        rest = &rest[k + 1..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    out.push_str(rest);
}

/// Pushes `raw` with each literal whitespace turned into a space.
fn push_spaces(raw: &str, out: &mut String) {
    out.push_str(&raw.replace("\r\n", " ").replace(['\t', '\n', '\r'], " "));
}

/// Resolves a reference name (the text between `&` and `;`) to its character.
/// A signed number and NUL, which XML 1.0 §4.1 has no reference for, resolve
/// to nothing.
fn resolve_reference(name: &str) -> Option<char> {
    let code = match name.as_bytes() {
        b"amp" => return Some('&'),
        b"lt" => return Some('<'),
        b"gt" => return Some('>'),
        b"quot" => return Some('"'),
        b"apos" => return Some('\''),
        [b'#', b'x', digits @ ..] if digits.iter().all(u8::is_ascii_hexdigit) => {
            u32::from_str_radix(&name[2..], 16).ok()?
        }
        [b'#', digits @ ..] if digits.iter().all(u8::is_ascii_digit) => name[1..].parse().ok()?,
        _ => return None,
    };

    char::from_u32(code).filter(|&ch| ch != '\0')
}

/// The local part of a qualified name, cut at the first colon.
pub(crate) fn local(name: &str) -> &str {
    name.split_once(':').map_or(name, |(_, local)| local)
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec::Vec};

    use crate::rfc4918::xml::*;

    fn events(xml: &str) -> Vec<XmlEvent<'_>> {
        XmlScanner::new(xml).collect()
    }

    fn decoded(raw: &str) -> String {
        let mut out = String::new();
        decode_text(raw, 0, &mut out);
        out
    }

    #[test]
    fn scanner_skips_a_doctype_with_its_internal_subset() {
        let xml = "<!DOCTYPE a [<!ENTITY e \"x]>y\">]><!-- c --><a>&e;</a>";
        assert_eq!(
            events(xml),
            [
                XmlEvent::Start {
                    prefix: "",
                    local: "a",
                    attrs: "",
                    empty: false,
                },
                XmlEvent::Text {
                    raw: "&e;",
                    clean: 0,
                },
                XmlEvent::End("a"),
            ]
        );
    }

    #[test]
    fn scanner_ends_on_unterminated_markup() {
        let open = XmlEvent::Start {
            prefix: "",
            local: "a",
            attrs: "",
            empty: false,
        };
        assert_eq!(events("<a><!-- open"), [open]);
        assert_eq!(events("<a><b x=\"open"), [open]);
        assert_eq!(events("<a>< b/>"), [open]);
        assert_eq!(
            events("<a>tail"),
            [
                open,
                XmlEvent::Text {
                    raw: "tail",
                    clean: 4,
                },
            ]
        );
    }

    #[test]
    fn scanner_reads_names_past_one_probe() {
        let xml = "<C:supported-calendar-component-set a:b='>'/>\
                   </d:supported-calendar-component-set\u{a0}>\
                   <x:y:zzzzzzzzzzzzzzz></a:b:c >";
        assert_eq!(
            events(xml),
            [
                XmlEvent::Start {
                    prefix: "C",
                    local: "supported-calendar-component-set",
                    attrs: " a:b='>'",
                    empty: true,
                },
                XmlEvent::End("supported-calendar-component-set"),
                XmlEvent::Start {
                    prefix: "x",
                    local: "y:zzzzzzzzzzzzzzz",
                    attrs: "",
                    empty: false,
                },
                XmlEvent::End("b:c"),
            ]
        );
        assert_eq!(events("<abcdefghijklmnopqrstu"), []);
    }

    #[test]
    fn scanner_tells_the_clean_prefix_of_text() {
        let xml = "<a>0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef&amp;\r<b>";
        let XmlEvent::Text { raw, clean } = events(xml)[1] else {
            panic!("no text");
        };
        assert_eq!((raw.len(), clean), (70, 64));
    }

    #[test]
    fn attr_reads_an_empty_prefix_as_none() {
        assert_eq!(attr(" x:name='a' :name='b'", "name"), Some("b"));
        assert_eq!(attr(" name='a' x:name='b'", "name"), Some("a"));
        assert_eq!(attr(" x:name='a'", "name"), None);
    }

    #[test]
    fn attrs_stop_at_an_unquoted_value() {
        let attrs: Vec<_> = XmlAttrs(" a='1' b=2 c=\"3\"").collect();
        assert_eq!(attrs, [("a", "1")]);
    }

    #[test]
    fn same_name_compares_every_length() {
        let names = [
            "",
            "a",
            "ab",
            "abc",
            "abcd",
            "abcdefg",
            "abcdefgh",
            "abcdefghijklmnop",
        ];
        for name in names {
            assert!(same_name(name, name));
            for i in 0..name.len() {
                let mut other = String::from(name);
                other.replace_range(i..=i, "_");
                assert!(!same_name(name, &other), "{name} {other}");
            }
        }
        let long = "supported-calendar-component-set";
        assert!(same_name(long, long));
        assert!(!same_name(long, "supported-calendar-component-sex"));
        assert!(!same_name("abc", "abcd"));
    }

    #[test]
    fn decode_attr_spaces_whitespace_and_keeps_unknown_references() {
        let decoded = decode_attr("a\r\nb\tc&bo\rgus;&amp;\rd&open\n");
        assert_eq!(decoded, "a b c&bo gus;& d&open ");
    }

    #[test]
    fn decode_text_normalises_line_ends_inside_unknown_references() {
        assert_eq!(decoded("x&a\r\nb;y&#13;\r&open\r"), "x&a\nb;y\r\n&open\n");
    }

    #[test]
    fn decode_text_resolves_references_and_keeps_unknown_ones() {
        let line = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+";
        let raw = [
            line,
            "&#13;\r\n",
            line,
            "&amp;&lt;&gt;&quot;&apos;&#x41;&#66;&#0067;\r",
            line,
        ];
        let want = [line, "\r\n", line, "&<>\"'ABC\n", line];
        assert_eq!(decoded(&raw.concat()), want.concat());
        assert_eq!(decoded("é&#xE9;&#233;&#x1F600;"), "ééé😀");
        assert_eq!(
            decoded("a&#xD800;b&#99999999999;c&#;d&#x;e&X;"),
            "a&#xD800;b&#99999999999;c&#;d&#x;e&X;"
        );
    }

    #[test]
    fn references_to_nul_and_signed_references_stay_verbatim() {
        assert_eq!(decoded("a&#0;b&#x0;c"), "a&#0;b&#x0;c");
        assert_eq!(decoded("&#x+41;&#+65;&#-0;"), "&#x+41;&#+65;&#-0;");
        assert_eq!(decode_attr("&#0;&#x+41;"), "&#0;&#x+41;");
    }

    #[test]
    fn decode_text_appends_after_its_clean_prefix() {
        let mut out = String::from("pre ");
        decode_text("clean &amp; dirty\r\n", 6, &mut out);
        assert_eq!(out, "pre clean & dirty\n");
    }
}
