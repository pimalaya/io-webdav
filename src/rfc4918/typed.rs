//! # Typed multistatus
//!
//! A `multistatus` body whose properties deserialize into a caller's serde
//! type, next to the vocabulary-agnostic
//! [`WebdavMultistatus`](crate::rfc4918::WebdavMultistatus). The envelope
//! (href, statuses, sync token, refused properties) keeps its semantics; the
//! 2xx properties of each response deserialize into `P`, read as follows:
//!
//! - children match fields by local name, prefixes ignored, the first of
//!   several same-named runs winning;
//! - a text element reads as a string, a number or a boolean, trimmed;
//! - an element reads as a struct or a map of its children, its attributes
//!   first under `@` keys (`@name`), consecutive same-named children filling
//!   a sequence field;
//! - an empty element reads as unit, so `Option<()>` flags its presence;
//! - a tuple reads the children of an element, an enum its first child by
//!   name, and a sequence below the field level its children, enums by name.
//!
//! [`WebdavPropfind`](crate::rfc4918::propfind::WebdavPropfind) and
//! [`WebdavReport`](crate::rfc4918::report::WebdavReport) parse into a
//! [`WebdavMultistatusOf`] when asked to; [`parse_multistatus_as`] parses a
//! body at hand, borrowing from it where `P` does.

use core::{cell::Cell, fmt, mem};

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use serde::{
    Deserialize, Deserializer,
    de::{
        self, DeserializeOwned, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess,
        Visitor,
    },
    forward_to_deserialize_any,
};

use crate::rfc4918::{
    MULTISTATUS_MAX_DEPTH, WebdavMultistatusParse, WebdavMultistatusParseError, WebdavPropFailure,
    sealed::Sealed,
    status_code,
    xml::{XmlAttrs, XmlEvent, XmlScanner, decode_attr, decode_text, local},
};

/// Parsed `multistatus` body (RFC 4918 §14.16) whose responses carry typed
/// properties.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavMultistatusOf<P> {
    /// The parsed `<response>` entries.
    pub responses: Vec<WebdavResponseOf<P>>,
    /// The top-level `DAV:sync-token` of a `sync-collection` REPORT (RFC 6578
    /// §6.2); [`None`] outside sync responses.
    pub sync_token: Option<String>,
}

/// A `<response>` inside a [`WebdavMultistatusOf`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavResponseOf<P> {
    /// The first `<href>` text, trimmed.
    pub href: String,
    /// The response-level `<status>` code, when present.
    pub status: Option<u16>,
    /// The properties of every 2xx `<propstat>`, deserialized together.
    pub props: P,
    /// The properties the server refused, with their `<propstat>` status.
    pub failures: Vec<WebdavPropFailure>,
}

impl<P> WebdavResponseOf<P> {
    /// Returns the last non-empty path segment of [`href`](Self::href).
    pub fn id(&self) -> &str {
        self.href
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
    }
}

impl<P> Sealed for WebdavMultistatusOf<P> {}

impl<P: DeserializeOwned> WebdavMultistatusParse for WebdavMultistatusOf<P> {
    fn parse(xml: &str) -> Result<Self, WebdavMultistatusParseError> {
        parse_multistatus_as(xml)
    }
}

/// Parses a `multistatus` body, deserializing the 2xx properties of each
/// response into `P`.
///
/// Malformed input yields its prefix, nesting past [`MULTISTATUS_MAX_DEPTH`]
/// fails, and so does a response whose properties do not fit `P`: a listing
/// missing a member would read as its removal.
pub fn parse_multistatus_as<'a, P: Deserialize<'a>>(
    xml: &'a str,
) -> Result<WebdavMultistatusOf<P>, WebdavMultistatusParseError> {
    let mut out = WebdavMultistatusOf {
        responses: Vec::new(),
        sync_token: None,
    };
    let mut frames: Vec<(Frame, &str)> = Vec::with_capacity(16);
    let mut nodes: Vec<Node<'a>> = Vec::new();
    let mut text: Cow<'a, str> = Cow::Borrowed("");
    let mut href: Option<String> = None;
    let mut status = None;
    let mut failures = Vec::new();
    let mut propstat_status = None;
    let mut propstat_root = NONE;

    for event in XmlScanner::new(xml) {
        match event {
            XmlEvent::Start {
                local,
                attrs,
                empty,
            } => {
                let frame = match frames.last().map(|(frame, _)| frame) {
                    None if local == "multistatus" => Frame::Multistatus,
                    Some(Frame::Multistatus) => match local {
                        "response" => Frame::Response,
                        "sync-token" => Frame::Text(Field::SyncToken),
                        _ => Frame::Skip,
                    },
                    Some(Frame::Response) => match local {
                        "href" => Frame::Text(Field::Href),
                        "status" => Frame::Text(Field::Status),
                        "propstat" => Frame::Propstat,
                        _ => Frame::Skip,
                    },
                    Some(Frame::Propstat) => match local {
                        "status" => Frame::Text(Field::PropstatStatus),
                        "prop" => Frame::Node(propstat_root),
                        _ => Frame::Skip,
                    },
                    Some(&Frame::Node(parent)) => {
                        Frame::Node(push_node(&mut nodes, parent, local, attrs))
                    }
                    _ => Frame::Skip,
                };

                match frame {
                    Frame::Response => {
                        nodes.clear();
                        nodes.push(Node::new("prop", ""));
                        href = None;
                        status = None;
                        failures.clear();
                    }
                    Frame::Propstat => {
                        propstat_status = None;
                        propstat_root = nodes.len() as u32;
                        nodes.push(Node::new("prop", ""));
                    }
                    Frame::Text(_) => text = Cow::Borrowed(""),
                    _ => {}
                }

                if !empty {
                    if frames.len() == MULTISTATUS_MAX_DEPTH {
                        let err = WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH);
                        return Err(err);
                    }
                    frames.push((frame, local));
                }
            }
            XmlEvent::Text { raw, clean } => match frames.last().map(|(frame, _)| frame) {
                Some(Frame::Text(_)) => push_text(&mut text, raw, clean),
                Some(&Frame::Node(i)) if i != propstat_root => {
                    let mut own = nodes[i as usize].text.take();
                    push_text(&mut own, raw, clean);
                    nodes[i as usize].text.set(own);
                }
                _ => {}
            },
            XmlEvent::Cdata(raw) => match frames.last().map(|(frame, _)| frame) {
                Some(Frame::Text(_)) => push_text(&mut text, raw, true),
                Some(&Frame::Node(i)) if i != propstat_root => {
                    let mut own = nodes[i as usize].text.take();
                    push_text(&mut own, raw, true);
                    nodes[i as usize].text.set(own);
                }
                _ => {}
            },
            XmlEvent::End(close) => {
                let Some((frame, _)) = frames.pop_if(|(_, local)| *local == close) else {
                    break;
                };

                match frame {
                    Frame::Text(Field::Href) if href.is_none() => {
                        href = Some(text.trim().to_string());
                    }
                    Frame::Text(Field::Status) => status = status_code(&text),
                    Frame::Text(Field::PropstatStatus) => propstat_status = status_code(&text),
                    Frame::Text(Field::SyncToken) => {
                        let token = text.trim();
                        if !token.is_empty() {
                            out.sync_token = Some(token.to_string());
                        }
                    }
                    Frame::Propstat => match propstat_status {
                        Some(code) if code / 100 == 2 => splice(&mut nodes, 0, propstat_root),
                        Some(code) => {
                            let mut child = nodes[propstat_root as usize].first;
                            while child != NONE {
                                failures.push(WebdavPropFailure {
                                    status: code,
                                    property: nodes[child as usize].local.to_string(),
                                });
                                child = nodes[child as usize].next;
                            }
                        }
                        None => {}
                    },
                    Frame::Response => {
                        let href = href.take().unwrap_or_default();
                        let root = NodeDe {
                            nodes: &nodes,
                            i: 0,
                        };
                        let props = match P::deserialize(root) {
                            Ok(props) => props,
                            Err(DeError(reason)) => {
                                let err =
                                    WebdavMultistatusParseError::InvalidProps { href, reason };
                                return Err(err);
                            }
                        };

                        out.responses.push(WebdavResponseOf {
                            href,
                            status: status.take(),
                            props,
                            failures: mem::take(&mut failures),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(out)
}

const NONE: u32 = u32::MAX;

/// What an open element is to the envelope.
#[derive(Clone, Copy, Debug)]
enum Frame {
    Multistatus,
    Response,
    Propstat,
    Text(Field),
    Node(u32),
    Skip,
}

/// An envelope element read as text.
#[derive(Clone, Copy, Debug)]
enum Field {
    Href,
    Status,
    PropstatStatus,
    SyncToken,
}

/// A property element, linked to its first child and its next sibling.
struct Node<'a> {
    local: &'a str,
    attrs: &'a str,
    text: Cell<Cow<'a, str>>,
    first: u32,
    last: u32,
    next: u32,
}

impl<'a> Node<'a> {
    fn new(local: &'a str, attrs: &'a str) -> Self {
        Self {
            local,
            attrs,
            text: Cell::new(Cow::Borrowed("")),
            first: NONE,
            last: NONE,
            next: NONE,
        }
    }

    /// Takes the trimmed text out, read once.
    fn take_text(&self) -> Cow<'a, str> {
        match self.text.take() {
            Cow::Borrowed(text) => Cow::Borrowed(text.trim()),
            Cow::Owned(mut text) => {
                text.truncate(text.trim_end().len());
                text.drain(..text.len() - text.trim_start().len());
                Cow::Owned(text)
            }
        }
    }

    fn is_leaf(&self) -> bool {
        self.first == NONE && XmlAttrs(self.attrs).all(|(name, _)| is_xmlns(name))
    }
}

fn push_node<'a>(nodes: &mut Vec<Node<'a>>, parent: u32, local: &'a str, attrs: &'a str) -> u32 {
    let i = nodes.len() as u32;
    nodes.push(Node::new(local, attrs));

    match nodes[parent as usize].last {
        NONE => nodes[parent as usize].first = i,
        last => nodes[last as usize].next = i,
    }
    nodes[parent as usize].last = i;

    i
}

/// Moves the children of `from` to the end of `to`'s.
fn splice(nodes: &mut [Node], to: u32, from: u32) {
    let (first, last) = (nodes[from as usize].first, nodes[from as usize].last);
    if first == NONE {
        return;
    }

    match nodes[to as usize].last {
        NONE => nodes[to as usize].first = first,
        tail => nodes[tail as usize].next = first,
    }
    nodes[to as usize].last = last;
}

/// Appends a text piece, skipping the indentation before any content, which
/// trimming drops anyway.
fn push_text<'a>(text: &mut Cow<'a, str>, raw: &'a str, clean: bool) {
    if text.is_empty() && raw.bytes().all(|b| b.is_ascii_whitespace()) {
        return;
    }

    if !clean {
        decode_text(raw, text.to_mut());
    } else if text.is_empty() {
        *text = Cow::Borrowed(raw);
    } else {
        text.to_mut().push_str(raw);
    }
}

fn is_xmlns(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

/// A deserialization failure, by message.
#[derive(Debug)]
struct DeError(String);

impl fmt::Display for DeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::error::Error for DeError {}

impl de::Error for DeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DeError(msg.to_string())
    }
}

/// One element.
#[derive(Clone, Copy)]
struct NodeDe<'n, 'a> {
    nodes: &'n [Node<'a>],
    i: u32,
}

impl<'n, 'a> NodeDe<'n, 'a> {
    fn node(&self) -> &'n Node<'a> {
        &self.nodes[self.i as usize]
    }

    fn visit_text<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.node().take_text() {
            Cow::Borrowed(text) => visitor.visit_borrowed_str(text),
            Cow::Owned(text) => visitor.visit_string(text),
        }
    }

    fn parse<T: core::str::FromStr>(self) -> Result<T, DeError> {
        let text = self.node().take_text();
        text.parse().map_err(|_| {
            let local = self.node().local;
            de::Error::custom(format_args!("invalid number in {local}: {text}"))
        })
    }
}

macro_rules! parse_number {
    ($($method:ident $visit:ident $ty:ty),*) => {$(
        fn $method<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
            visitor.$visit(self.parse::<$ty>()?)
        }
    )*};
}

impl<'n, 'a> Deserializer<'a> for NodeDe<'n, 'a> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        if !self.node().is_leaf() {
            return self.deserialize_map(visitor);
        }

        match self.node().take_text() {
            text if text.is_empty() => visitor.visit_unit(),
            Cow::Borrowed(text) => visitor.visit_borrowed_str(text),
            Cow::Owned(text) => visitor.visit_string(text),
        }
    }

    fn deserialize_str<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.visit_text(visitor)
    }

    fn deserialize_string<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.visit_text(visitor)
    }

    fn deserialize_char<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.visit_text(visitor)
    }

    fn deserialize_bool<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        match &*self.node().take_text() {
            "true" | "1" => visitor.visit_bool(true),
            "false" | "0" => visitor.visit_bool(false),
            text => Err(de::Error::custom(format_args!("invalid boolean: {text}"))),
        }
    }

    parse_number! {
        deserialize_u8 visit_u8 u8, deserialize_u16 visit_u16 u16,
        deserialize_u32 visit_u32 u32, deserialize_u64 visit_u64 u64,
        deserialize_i8 visit_i8 i8, deserialize_i16 visit_i16 i16,
        deserialize_i32 visit_i32 i32, deserialize_i64 visit_i64 i64,
        deserialize_f32 visit_f32 f32, deserialize_f64 visit_f64 f64
    }

    fn deserialize_option<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_seq(Siblings {
            nodes: self.nodes,
            cur: self.node().first,
            left: usize::MAX,
        })
    }

    fn deserialize_tuple<V: Visitor<'a>>(self, _: usize, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        _: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_map(Children {
            nodes: self.nodes,
            attrs: XmlAttrs(self.node().attrs),
            cur: self.node().first,
            seen: Vec::new(),
            value: Value::None,
        })
    }

    fn deserialize_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'a>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_enum(self)
    }

    fn deserialize_identifier<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_borrowed_str(self.node().local)
    }

    fn deserialize_ignored_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! { <W: Visitor<'a>> bytes byte_buf }
}

impl<'n, 'a> EnumAccess<'a> for NodeDe<'n, 'a> {
    type Error = DeError;
    type Variant = Self;

    fn variant_seed<S: DeserializeSeed<'a>>(self, seed: S) -> Result<(S::Value, Self), DeError> {
        let variant = seed.deserialize(Key(self.node().local))?;
        Ok((variant, self))
    }
}

impl<'n, 'a> VariantAccess<'a> for NodeDe<'n, 'a> {
    type Error = DeError;

    fn unit_variant(self) -> Result<(), DeError> {
        Ok(())
    }

    fn newtype_variant_seed<S: DeserializeSeed<'a>>(self, seed: S) -> Result<S::Value, DeError> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'a>>(self, _: usize, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn struct_variant<V: Visitor<'a>>(
        self,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_map(visitor)
    }
}

/// A run of consecutive same-named siblings, the value of one map key: a
/// sequence reads them all, anything else reads the first.
struct Group<'n, 'a> {
    first: NodeDe<'n, 'a>,
    len: usize,
}

macro_rules! forward_to_first {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
            self.first.$method(visitor)
        }
    )*};
}

impl<'n, 'a> Deserializer<'a> for Group<'n, 'a> {
    type Error = DeError;

    forward_to_first! {
        deserialize_any deserialize_bool deserialize_u8 deserialize_u16
        deserialize_u32 deserialize_u64 deserialize_i8 deserialize_i16
        deserialize_i32 deserialize_i64 deserialize_f32 deserialize_f64
        deserialize_char deserialize_str deserialize_string deserialize_bytes
        deserialize_byte_buf deserialize_unit deserialize_map
        deserialize_identifier
    }

    fn deserialize_ignored_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        trace!(
            "ignoring unrecognized WebDAV element {}",
            self.first.node().local
        );
        visitor.visit_unit()
    }

    fn deserialize_option<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_some(self)
    }

    fn deserialize_seq<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_seq(Siblings {
            nodes: self.first.nodes,
            cur: self.first.i,
            left: self.len,
        })
    }

    fn deserialize_tuple<V: Visitor<'a>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.first.deserialize_tuple(len, visitor)
    }

    fn deserialize_unit_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.first.deserialize_unit_struct(name, visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.first.deserialize_newtype_struct(name, visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.first.deserialize_tuple_struct(name, len, visitor)
    }

    fn deserialize_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.first.deserialize_struct(name, fields, visitor)
    }

    fn deserialize_enum<V: Visitor<'a>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        let local = self.first.node().local;
        let i = self.first.node().first;
        if i == NONE {
            let msg = format_args!("{local} names no variant: it has no child element");
            return Err(de::Error::custom(msg));
        }

        visitor.visit_enum(NodeDe {
            nodes: self.first.nodes,
            i,
        })
    }
}

/// Sibling elements as a sequence, `left` of them at most.
struct Siblings<'n, 'a> {
    nodes: &'n [Node<'a>],
    cur: u32,
    left: usize,
}

impl<'n, 'a> SeqAccess<'a> for Siblings<'n, 'a> {
    type Error = DeError;

    fn next_element_seed<S: DeserializeSeed<'a>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, DeError> {
        if self.cur == NONE || self.left == 0 {
            return Ok(None);
        }

        let node = NodeDe {
            nodes: self.nodes,
            i: self.cur,
        };
        self.cur = self.nodes[self.cur as usize].next;
        self.left -= 1;

        seed.deserialize(node).map(Some)
    }
}

/// The value of the key a [`Children`] map last returned.
enum Value<'n, 'a> {
    None,
    Attr(&'a str),
    Group(Group<'n, 'a>),
}

/// An element's attributes, then its children grouped by name, as a map.
struct Children<'n, 'a> {
    nodes: &'n [Node<'a>],
    attrs: XmlAttrs<'a>,
    cur: u32,
    seen: Vec<&'a str>,
    value: Value<'n, 'a>,
}

impl<'n, 'a> MapAccess<'a> for Children<'n, 'a> {
    type Error = DeError;

    fn next_key_seed<K: DeserializeSeed<'a>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, DeError> {
        for (name, value) in self.attrs.by_ref() {
            if is_xmlns(name) {
                continue;
            }

            let local = local(name);
            let mut key = String::with_capacity(local.len() + 1);
            key.push('@');
            key.push_str(local);

            self.value = Value::Attr(value);
            return seed.deserialize(OwnedKey(key)).map(Some);
        }

        loop {
            if self.cur == NONE {
                return Ok(None);
            }

            let first = self.cur;
            let local = self.nodes[first as usize].local;
            let mut len = 0;
            while self.cur != NONE && self.nodes[self.cur as usize].local == local {
                self.cur = self.nodes[self.cur as usize].next;
                len += 1;
            }

            if self.seen.contains(&local) {
                continue;
            }
            self.seen.push(local);

            let first = NodeDe {
                nodes: self.nodes,
                i: first,
            };
            self.value = Value::Group(Group { first, len });
            return seed.deserialize(Key(local)).map(Some);
        }
    }

    fn next_value_seed<S: DeserializeSeed<'a>>(&mut self, seed: S) -> Result<S::Value, DeError> {
        match mem::replace(&mut self.value, Value::None) {
            Value::Attr(raw) => seed.deserialize(Attr(raw)),
            Value::Group(group) => seed.deserialize(group),
            Value::None => Err(de::Error::custom(
                "WebDAV property value read before its key",
            )),
        }
    }
}

/// A borrowed map key or enum variant name.
struct Key<'a>(&'a str);

impl<'a> Deserializer<'a> for Key<'a> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_borrowed_str(self.0)
    }

    forward_to_deserialize_any! {
        <W: Visitor<'a>>
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

/// An attribute key, `@` then its local name.
struct OwnedKey(String);

impl<'a> Deserializer<'a> for OwnedKey {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_string(self.0)
    }

    forward_to_deserialize_any! {
        <W: Visitor<'a>>
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

/// A raw attribute value.
struct Attr<'a>(&'a str);

impl<'a> Deserializer<'a> for Attr<'a> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        if self.0.contains(['&', '\t', '\n', '\r']) {
            visitor.visit_string(decode_attr(self.0))
        } else {
            visitor.visit_borrowed_str(self.0)
        }
    }

    fn deserialize_option<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_some(self)
    }

    forward_to_deserialize_any! {
        <W: Visitor<'a>>
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

    use serde::Deserialize;

    use crate::rfc4918::{
        MULTISTATUS_MAX_DEPTH, WebdavMultistatusParseError, WebdavPropFailure, typed::*,
    };

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    struct Props<'a> {
        getetag: Option<&'a str>,
        displayname: Option<String>,
        calendar_color: Option<String>,
        calendar_data: Option<String>,
        resourcetype: Option<Kinds>,
        supported_calendar_component_set: Option<Comps>,
        supported_report_set: Option<Reports>,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct Kinds(Vec<Kind>);

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    enum Kind {
        Collection,
        Calendar,
        #[serde(other)]
        Other,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct Comps {
        comp: Vec<Comp>,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct Comp {
        #[serde(rename = "@name")]
        name: String,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    struct Reports {
        supported_report: Vec<Report>,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct Report {
        report: BTreeMap<String, ()>,
    }

    const BODY: &str = "<?xml version=\"1.0\"?>
<d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">
 <d:response>
  <d:href> /cal/ </d:href>
  <d:propstat>
   <d:prop>
    <d:displayname>R&amp;D</d:displayname>
    <d:resourcetype>
     <d:collection/>
     <c:calendar/>
     <x:shared xmlns:x=\"urn:x\"/>
    </d:resourcetype>
    <c:supported-calendar-component-set>
     <c:comp name=\"VEVENT\"/>
     <c:comp name='VTODO'/>
    </c:supported-calendar-component-set>
    <d:supported-report-set>
     <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>
     <d:supported-report><d:report><c:calendar-multiget/></d:report></d:supported-report>
    </d:supported-report-set>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat>
   <d:prop><d:displayname>shadowed</d:displayname><c:calendar-color/></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat>
   <d:prop><c:calendar-timezone/></d:prop>
   <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/cal/a.ics</d:href>
  <d:propstat>
   <d:prop>
    <d:getetag>\"1\"</d:getetag>
    <c:calendar-data>BEGIN:VCALENDAR&#13;\nEND:VCALENDAR\r\n</c:calendar-data>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/cal/gone.ics</d:href>
  <d:status>HTTP/1.1 404 Not Found</d:status>
 </d:response>
 <d:sync-token>tok</d:sync-token>
</d:multistatus>";

    #[test]
    fn parse_multistatus_as_reads_props_into_the_given_type() {
        let ms = parse_multistatus_as::<Props>(BODY).unwrap();
        assert_eq!(ms.sync_token.as_deref(), Some("tok"));
        assert_eq!(ms.responses.len(), 3);

        let cal = &ms.responses[0];
        assert_eq!(cal.href, "/cal/");
        assert_eq!(cal.props.displayname.as_deref(), Some("R&D"));
        assert_eq!(cal.props.calendar_color.as_deref(), Some(""));
        assert_eq!(
            cal.props.resourcetype,
            Some(Kinds(vec![Kind::Collection, Kind::Calendar, Kind::Other]))
        );
        let comps = &cal
            .props
            .supported_calendar_component_set
            .as_ref()
            .unwrap()
            .comp;
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[1].name, "VTODO");
        let reports = &cal
            .props
            .supported_report_set
            .as_ref()
            .unwrap()
            .supported_report;
        assert!(reports[0].report.contains_key("sync-collection"));
        assert!(reports[1].report.contains_key("calendar-multiget"));
        assert_eq!(
            cal.failures,
            vec![WebdavPropFailure {
                status: 404,
                property: "calendar-timezone".into(),
            }]
        );

        let item = &ms.responses[1];
        assert_eq!(item.id(), "a.ics");
        assert_eq!(item.props.getetag, Some("\"1\""));
        assert_eq!(
            item.props.calendar_data.as_deref(),
            Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR")
        );

        let gone = &ms.responses[2];
        assert_eq!(gone.status, Some(404));
        assert_eq!(gone.props, Props::default());
    }

    #[test]
    fn parse_multistatus_as_borrows_text_needing_no_decoding() {
        let ms = parse_multistatus_as::<Props>(BODY).unwrap();
        let etag = ms.responses[1].props.getetag.unwrap();
        let body = BODY.as_bytes().as_ptr_range();
        assert!(body.contains(&etag.as_ptr()));
    }

    #[test]
    fn parse_multistatus_as_fails_on_props_not_fitting_the_type() {
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Number {
            displayname: Option<u32>,
        }

        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Boolean {
            displayname: Option<bool>,
        }

        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        #[serde(rename_all = "kebab-case")]
        struct Variant {
            calendar_color: Option<Kind>,
        }

        for err in [
            parse_multistatus_as::<Number>(BODY).unwrap_err(),
            parse_multistatus_as::<Boolean>(BODY).unwrap_err(),
            parse_multistatus_as::<Variant>(BODY).unwrap_err(),
        ] {
            let WebdavMultistatusParseError::InvalidProps { href, .. } = err else {
                panic!("unexpected error {err:?}");
            };
            assert_eq!(href, "/cal/");
        }
    }

    #[test]
    fn parse_multistatus_as_caps_the_nesting_depth() {
        let body = "<a>".repeat(MULTISTATUS_MAX_DEPTH + 1);
        let err = parse_multistatus_as::<Props>(&body).unwrap_err();
        assert_eq!(
            err,
            WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH)
        );
    }

    #[test]
    fn parse_multistatus_as_yields_the_prefix_of_a_truncated_body() {
        let cut = BODY.find("<d:response>\n  <d:href>/cal/gone.ics").unwrap() + 20;
        let ms = parse_multistatus_as::<Props>(&BODY[..cut]).unwrap();
        assert_eq!(ms.responses.len(), 2);
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    struct Scalars {
        getcontentlength: u64,
        quota_used_bytes: i32,
        ratio: f64,
        enabled: bool,
        disabled: bool,
        initial: char,
        pair: (String, Option<String>),
        couple: Couple,
        names: Names,
        skipped: Vec<serde::de::IgnoredAny>,
        any: serde_json::Value,
        marker: Marker,
        kind: Shape,
        shapes: Shapes,
        link: Link,
        notes: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Couple(String, String);

    #[derive(Debug, Deserialize, PartialEq)]
    struct Names(Vec<Name>);

    /// An element read as an identifier: its local name.
    #[derive(Debug, PartialEq)]
    struct Name(String);

    impl<'de> Deserialize<'de> for Name {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_identifier(NameVisitor)
        }
    }

    struct NameVisitor;

    impl<'de> serde::de::Visitor<'de> for NameVisitor {
        type Value = Name;

        fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            f.write_str("an element name")
        }

        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Name, E> {
            Ok(Name(v.into()))
        }
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Marker;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Shapes(Vec<Shape>);

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum Shape {
        Circle,
        Square(String),
        Rect { w: u8, h: u8 },
        Line(u8, u8),
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Link {
        #[serde(rename = "@title")]
        title: String,
        #[serde(rename = "@rel")]
        rel: Option<String>,
    }

    #[test]
    fn parse_multistatus_as_reads_scalars_tuples_enums_and_attributes() {
        let body = "<d:multistatus xmlns:d=\"DAV:\">
 <d:responsedescription>ignored</d:responsedescription>
 <![CDATA[ignored]]>
 <d:response>
  <d:href><![CDATA[/a]]></d:href>
  <d:location><d:href>/elsewhere</d:href></d:location>
  <d:propstat>
   <d:prop>
    <d:getcontentlength> 42 </d:getcontentlength>
    <d:quota-used-bytes>-7</d:quota-used-bytes>
    <d:ratio>0.5</d:ratio>
    <d:enabled>1</d:enabled>
    <d:disabled>false</d:disabled>
    <d:initial>é</d:initial>
    <d:pair><d:a>x</d:a><d:b>y</d:b></d:pair>
    <d:couple><d:a>u</d:a><d:b>v</d:b></d:couple>
    <d:names><d:first/><d:second>2</d:second></d:names>
    <d:skipped>a</d:skipped><d:skipped>b</d:skipped>
    <d:any><d:leaf>t</d:leaf><d:amp>&amp;</d:amp><d:empty/><d:tree k=\"v\"/></d:any>
    <d:unknown>x</d:unknown>
    <d:marker/>
    <d:kind><d:circle/></d:kind>
    <d:shapes><d:square>s</d:square><d:rect><d:w>2</d:w><d:h>3</d:h></d:rect><d:line><d:a>1</d:a><d:b>2</d:b></d:line></d:shapes>
    <d:link xmlns:x=\"urn:x\" title=\"a &amp; b\" x:rel='next'/>
    <d:notes>one<![CDATA[ <two> ]]>three</d:notes>
   </d:prop>
   <d:responsedescription>ignored</d:responsedescription>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
  <d:propstat><d:prop><d:orphan/></d:prop></d:propstat>
  <d:propstat><d:prop></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
 </d:response>
 <d:response><d:href>/b</d:x>
</d:multistatus>";

        let ms = parse_multistatus_as::<Scalars>(body).unwrap();
        assert_eq!(ms.responses.len(), 1);
        assert!(ms.responses[0].failures.is_empty());
        assert_eq!(
            ms.responses[0].props,
            Scalars {
                getcontentlength: 42,
                quota_used_bytes: -7,
                ratio: 0.5,
                enabled: true,
                disabled: false,
                initial: 'é',
                pair: ("x".into(), Some("y".into())),
                couple: Couple("u".into(), "v".into()),
                names: Names(vec![Name("first".into()), Name("second".into())]),
                skipped: vec![serde::de::IgnoredAny, serde::de::IgnoredAny],
                any: serde_json::json!({"leaf": "t", "amp": "&", "empty": null, "tree": {"@k": "v"}}),
                marker: Marker,
                kind: Shape::Circle,
                shapes: Shapes(vec![
                    Shape::Square("s".into()),
                    Shape::Rect { w: 2, h: 3 },
                    Shape::Line(1, 2),
                ]),
                link: Link {
                    title: "a & b".into(),
                    rel: Some("next".into()),
                },
                notes: "one <two> three".into(),
            }
        );
    }
}
