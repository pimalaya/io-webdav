//! # XML scanner
//!
//! The tokenizer under the [`crate::rfc4918`] multistatus parsers, for the
//! XML subset WebDAV servers send: elements, attributes, text, CDATA,
//! comments and processing instructions. A DOCTYPE is skipped, internal
//! subset included, its entities left unexpanded like any unknown reference.
//! Malformed markup ends the scan, so a parser keeps whatever came before.
//!
//! Delimiters are searched with memchr's vector kernels, and a text event
//! says whether it needs decoding, found by the same search that finds its
//! end.

use alloc::string::String;

use memchr::{memchr, memchr2, memchr3, memmem};

/// One markup event, borrowed from the body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum XmlEvent<'a> {
    /// A start tag, `empty` for `<a/>`; `attrs` is the raw attribute area.
    Start {
        local: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    /// An end tag.
    End(&'a str),
    /// Raw character data, `clean` when it holds neither `&` nor CR.
    Text { raw: &'a str, clean: bool },
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
}

impl<'a> Iterator for XmlScanner<'a> {
    type Item = XmlEvent<'a>;

    fn next(&mut self) -> Option<XmlEvent<'a>> {
        loop {
            let rest = &self.xml.as_bytes()[self.pos..];
            let first = *rest.first()?;
            let at = self.pos;

            if first != b'<' {
                let (end, clean) = match memchr3(b'<', b'&', b'\r', rest) {
                    Some(i) if rest[i] == b'<' => (i, true),
                    Some(i) => (
                        i + memchr(b'<', &rest[i..]).unwrap_or(rest.len() - i),
                        false,
                    ),
                    None => (rest.len(), true),
                };

                self.pos += end;
                let raw = &self.xml[at..at + end];
                return Some(XmlEvent::Text { raw, clean });
            }

            match rest.get(1)? {
                b'/' => {
                    let end = memchr(b'>', rest)?;
                    self.pos += end + 1;
                    let name = self.xml[at + 2..at + end].trim_end();
                    return Some(XmlEvent::End(local(name)));
                }
                b'?' => {
                    let end = memmem::find(&rest[2..], b"?>")?;
                    self.pos += 2 + end + 2;
                }
                b'!' if rest.starts_with(b"<!--") => {
                    let end = memmem::find(&rest[4..], b"-->")?;
                    self.pos += 4 + end + 3;
                }
                b'!' if rest.starts_with(b"<![CDATA[") => {
                    let end = memmem::find(&rest[9..], b"]]>")?;
                    self.pos += 9 + end + 3;
                    return Some(XmlEvent::Cdata(&self.xml[at + 9..at + 9 + end]));
                }
                b'!' => {
                    let mut subset = false;
                    let mut i = 2;
                    let end = loop {
                        match *rest.get(i)? {
                            quote @ (b'"' | b'\'') => i += 1 + memchr(quote, &rest[i + 1..])?,
                            b'[' => subset = true,
                            b']' => subset = false,
                            b'>' if !subset => break i,
                            _ => {}
                        }
                        i += 1;
                    };
                    self.pos += end + 1;
                }
                _ => {
                    let name_end = 1 + rest[1..]
                        .iter()
                        .position(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>'))?;
                    if name_end == 1 {
                        return None;
                    }

                    let mut i = name_end;
                    let end = loop {
                        let j = i + memchr3(b'>', b'"', b'\'', &rest[i..])?;
                        match rest[j] {
                            b'>' => break j,
                            quote => i = j + 2 + memchr(quote, &rest[j + 1..])?,
                        }
                    };

                    let empty = end > name_end && rest[end - 1] == b'/';
                    self.pos += end + 1;
                    return Some(XmlEvent::Start {
                        local: local(&self.xml[at + 1..at + name_end]),
                        attrs: &self.xml[at + name_end..at + end - empty as usize],
                        empty,
                    });
                }
            }
        }
    }
}

/// Iterator of `(qualified name, raw value)` over a raw attribute area,
/// namespace declarations included.
#[derive(Clone, Copy, Debug)]
pub(crate) struct XmlAttrs<'a>(pub(crate) &'a str);

impl<'a> Iterator for XmlAttrs<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
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

/// Returns the raw value of the unprefixed attribute `want`, the last one
/// when repeated.
pub(crate) fn attr<'a>(area: &'a str, want: &str) -> Option<&'a str> {
    XmlAttrs(area)
        .filter(|(name, _)| *name == want)
        .last()
        .map(|(_, value)| value)
}

/// Appends decoded character data: line ends normalised to LF (XML 1.0
/// §2.11) and references resolved, in one pass.
pub(crate) fn decode_text(raw: &str, out: &mut String) {
    let bytes = raw.as_bytes();
    let mut i = 0;

    while let Some(k) = memchr2(b'&', b'\r', &bytes[i..]) {
        let k = i + k;
        out.push_str(&raw[i..k]);

        if bytes[k] == b'\r' {
            out.push('\n');
            i = k + 1 + (bytes.get(k + 1) == Some(&b'\n')) as usize;
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

    out.push_str(&raw[i..]);
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
fn resolve_reference(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let code = name.strip_prefix('#')?;
            let code = match code.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
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

    #[test]
    fn scanner_skips_a_doctype_with_its_internal_subset() {
        let xml = "<!DOCTYPE a [<!ENTITY e \"x]>y\">]><!-- c --><a>&e;</a>";
        assert_eq!(
            events(xml),
            [
                XmlEvent::Start {
                    local: "a",
                    attrs: "",
                    empty: false,
                },
                XmlEvent::Text {
                    raw: "&e;",
                    clean: false,
                },
                XmlEvent::End("a"),
            ]
        );
    }

    #[test]
    fn scanner_ends_on_unterminated_markup() {
        let open = XmlEvent::Start {
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
                    clean: true,
                },
            ]
        );
    }

    #[test]
    fn attrs_stop_at_an_unquoted_value() {
        let attrs: Vec<_> = XmlAttrs(" a='1' b=2 c=\"3\"").collect();
        assert_eq!(attrs, [("a", "1")]);
    }

    #[test]
    fn decode_attr_spaces_whitespace_and_keeps_unknown_references() {
        let decoded = decode_attr("a\r\nb\tc&bo\rgus;&amp;\rd&open\n");
        assert_eq!(decoded, "a b c&bo gus;& d&open ");
    }

    #[test]
    fn decode_text_normalises_line_ends_inside_unknown_references() {
        let mut out = String::new();
        decode_text("x&a\r\nb;y&#13;\r&open\r", &mut out);
        assert_eq!(out, "x&a\nb;y\r\n&open\n");
    }
}
