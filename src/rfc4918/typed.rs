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
    collections::BTreeSet,
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
    xml::{XmlAttrs, XmlEvent, XmlScanner, decode_attr, decode_text, local, same_name},
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

/// A listed resource whose `<response>` carried no data, as the server
/// answered it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavUnreadableEntry {
    /// Resource id: the last path segment of [`href`](Self::href), verbatim.
    pub id: String,
    /// The `<href>` text, trimmed.
    pub href: String,
    /// The response-level `<status>` code, when present (a 403 or a 404).
    pub status: Option<u16>,
    /// The properties the server refused, with their `<propstat>` status.
    pub failures: Vec<WebdavPropFailure>,
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
/// A body that is not one whole `multistatus` document fails, as does nesting
/// past [`MULTISTATUS_MAX_DEPTH`] or a response whose properties do not fit
/// `P`: a listing missing a member would read as its removal.
pub fn parse_multistatus_as<'a, P: Deserialize<'a>>(
    xml: &'a str,
) -> Result<WebdavMultistatusOf<P>, WebdavMultistatusParseError> {
    let mut envelope = Envelope::new(xml);
    let mut responses = Vec::new();

    while envelope.next_response()? {
        let href = envelope.href.take().unwrap_or_default();
        let root = NodeDe {
            nodes: envelope.nodes.as_slice(),
            i: 0,
        };
        let props = match P::deserialize(root) {
            Ok(props) => props,
            Err(DeError(reason)) => {
                return Err(WebdavMultistatusParseError::InvalidProps { href, reason });
            }
        };

        responses.push(WebdavResponseOf {
            href,
            status: envelope.status.take(),
            props,
            failures: mem::take(&mut envelope.failures),
        });
    }

    Ok(WebdavMultistatusOf {
        responses,
        sync_token: envelope.sync_token,
    })
}

/// A `multistatus` body walked response by response, the generic part of the
/// parse left to the caller.
struct Envelope<'a> {
    events: XmlScanner<'a>,
    frames: Vec<(Frame, &'a str)>,
    /// The property elements of the response read last.
    nodes: Nodes<'a>,
    text: Cow<'a, str>,
    href: Option<String>,
    status: Option<u16>,
    failures: Vec<WebdavPropFailure>,
    propstat_status: Option<u16>,
    propstat_root: u32,
    sync_token: Option<String>,
    /// Whether the root was closed.
    closed: bool,
    /// The count of responses read.
    responses: usize,
}

impl<'a> Envelope<'a> {
    fn new(xml: &'a str) -> Self {
        Self {
            events: XmlScanner::new(xml),
            frames: Vec::with_capacity(16),
            nodes: Nodes::default(),
            text: Cow::Borrowed(""),
            href: None,
            status: None,
            failures: Vec::new(),
            propstat_status: None,
            propstat_root: NONE,
            sync_token: None,
            closed: false,
            responses: 0,
        }
    }

    /// Reads up to the end of the next response, returning whether there was
    /// one: its href, status, failures and property nodes are then at hand.
    /// Fails unless the body is one whole `multistatus` document.
    fn next_response(&mut self) -> Result<bool, WebdavMultistatusParseError> {
        let nodes = &mut self.nodes;

        for event in self.events.by_ref() {
            match event {
                XmlEvent::Start {
                    local,
                    attrs,
                    empty,
                } => {
                    let frame = match self.frames.last().map(|(frame, _)| frame) {
                        None if self.closed => {
                            let err = WebdavMultistatusParseError::Incomplete(self.responses);
                            return Err(err);
                        }
                        None if matches!(local.as_bytes(), b"multistatus") => Frame::Multistatus,
                        None => {
                            let root = local.to_string();
                            return Err(WebdavMultistatusParseError::UnexpectedRoot(root));
                        }
                        Some(Frame::Multistatus) => match local.as_bytes() {
                            b"response" => Frame::Response,
                            b"sync-token" => Frame::Text(Field::SyncToken),
                            _ => Frame::Skip,
                        },
                        Some(Frame::Response) => match local.as_bytes() {
                            b"href" => Frame::Text(Field::Href),
                            b"status" => Frame::Text(Field::Status),
                            b"propstat" => Frame::Propstat,
                            _ => Frame::Skip,
                        },
                        Some(Frame::Propstat) => match local.as_bytes() {
                            b"status" => Frame::Text(Field::PropstatStatus),
                            b"prop" => Frame::Node(self.propstat_root),
                            _ => Frame::Skip,
                        },
                        Some(&Frame::Node(parent)) => {
                            Frame::Node(nodes.add_child(parent, local, attrs))
                        }
                        _ => Frame::Skip,
                    };

                    match frame {
                        Frame::Response => {
                            nodes.clear();
                            nodes.add("prop", "");
                            self.href = None;
                            self.status = None;
                            self.failures.clear();
                        }
                        Frame::Propstat => {
                            self.propstat_status = None;
                            self.propstat_root = nodes.add("prop", "");
                        }
                        Frame::Text(_) => self.text = Cow::Borrowed(""),
                        _ => {}
                    }

                    if !empty {
                        if self.frames.len() == MULTISTATUS_MAX_DEPTH {
                            let err = WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH);
                            return Err(err);
                        }
                        self.frames.push((frame, local));
                    } else if self.frames.is_empty() {
                        self.closed = true;
                    }
                }
                XmlEvent::Text { raw, clean } => match self.frames.last().map(|(frame, _)| frame) {
                    Some(Frame::Text(_)) => push_text(&mut self.text, raw, clean, false),
                    Some(&Frame::Node(i)) if i != self.propstat_root => {
                        let node = &mut nodes.slots[i as usize];
                        let nested = node.first != NONE;
                        push_text(node.text.get_mut(), raw, clean, nested);
                    }
                    _ => {}
                },
                XmlEvent::Cdata(raw) => match self.frames.last().map(|(frame, _)| frame) {
                    Some(Frame::Text(_)) => push_text(&mut self.text, raw, raw.len(), false),
                    Some(&Frame::Node(i)) if i != self.propstat_root => {
                        let node = &mut nodes.slots[i as usize];
                        let nested = node.first != NONE;
                        push_text(node.text.get_mut(), raw, raw.len(), nested);
                    }
                    _ => {}
                },
                XmlEvent::End(close) => {
                    let frame = match self.frames.last() {
                        Some(&(frame, local)) if same_name(local, close) => frame,
                        _ => {
                            let err = WebdavMultistatusParseError::Incomplete(self.responses);
                            return Err(err);
                        }
                    };
                    self.frames.pop();

                    match frame {
                        Frame::Text(Field::Href) if self.href.is_none() => {
                            self.href = Some(self.text.trim().to_string());
                        }
                        Frame::Text(Field::Status) => self.status = status_code(&self.text),
                        Frame::Text(Field::PropstatStatus) => {
                            self.propstat_status = status_code(&self.text);
                        }
                        Frame::Text(Field::SyncToken) => {
                            let token = self.text.trim();
                            if !token.is_empty() {
                                self.sync_token = Some(token.to_string());
                            }
                        }
                        Frame::Propstat => match self.propstat_status {
                            Some(code) if code / 100 == 2 => nodes.splice(0, self.propstat_root),
                            Some(code) => {
                                let nodes = nodes.as_slice();
                                let mut child = nodes[self.propstat_root as usize].first;
                                while child != NONE {
                                    self.failures.push(WebdavPropFailure {
                                        status: code,
                                        property: nodes[child as usize].local.to_string(),
                                    });
                                    child = nodes[child as usize].next;
                                }
                            }
                            None => {}
                        },
                        Frame::Response => {
                            self.responses += 1;
                            return Ok(true);
                        }
                        Frame::Multistatus => self.closed = true,
                        _ => {}
                    }
                }
            }
        }

        if !self.closed || !self.events.is_done() {
            let err = WebdavMultistatusParseError::Incomplete(self.responses);
            return Err(err);
        }

        Ok(false)
    }
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

    /// Makes the node a new one, in place.
    fn reset(&mut self, local: &'a str, attrs: &'a str) {
        self.local = local;
        self.attrs = attrs;
        *self.text.get_mut() = Cow::Borrowed("");
        self.first = NONE;
        self.last = NONE;
        self.next = NONE;
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

    /// Takes the whole text out, read once.
    fn take_whole_text(&self) -> Cow<'a, str> {
        self.text.take()
    }

    fn is_leaf(&self) -> bool {
        self.first == NONE && XmlAttrs(self.attrs).all(|(name, _)| is_xmlns(name))
    }
}

/// The property elements of the response being read, in slots reused from
/// one response to the next: a node is reset in place rather than moved in.
#[derive(Default)]
struct Nodes<'a> {
    slots: Vec<Node<'a>>,
    len: usize,
}

impl<'a> Nodes<'a> {
    fn clear(&mut self) {
        self.len = 0;
    }

    fn as_slice(&self) -> &[Node<'a>] {
        &self.slots[..self.len]
    }

    /// Adds a node, returning its index.
    fn add(&mut self, local: &'a str, attrs: &'a str) -> u32 {
        match self.slots.get_mut(self.len) {
            Some(node) => node.reset(local, attrs),
            None => self.slots.push(Node::new(local, attrs)),
        }

        self.len += 1;
        self.len as u32 - 1
    }

    /// Adds a node as the last child of `parent`, returning its index.
    fn add_child(&mut self, parent: u32, local: &'a str, attrs: &'a str) -> u32 {
        let i = self.add(local, attrs);
        let slots = &mut self.slots;

        match slots[parent as usize].last {
            NONE => slots[parent as usize].first = i,
            last => slots[last as usize].next = i,
        }
        slots[parent as usize].last = i;

        i
    }

    /// Moves the children of `from` to the end of `to`'s.
    fn splice(&mut self, to: u32, from: u32) {
        let slots = &mut self.slots;
        let (first, last) = (slots[from as usize].first, slots[from as usize].last);
        if first == NONE {
            return;
        }

        match slots[to as usize].last {
            NONE => slots[to as usize].first = first,
            tail => slots[tail as usize].next = first,
        }
        slots[to as usize].last = last;
    }
}

/// Appends a text piece. With `nested`, the element holding child elements,
/// a blank piece is skipped while the text is blank too: indentation between
/// children, which trimming drops anyway. A leaf keeps every piece, so its
/// text can be read whole.
fn push_text<'a>(text: &mut Cow<'a, str>, raw: &'a str, clean: usize, nested: bool) {
    if nested && is_blank(raw) && is_blank(text) {
        return;
    }

    if clean < raw.len() {
        decode_text(raw, clean, text.to_mut());
    } else if text.is_empty() {
        *text = Cow::Borrowed(raw);
    } else {
        text.to_mut().push_str(raw);
    }
}

fn is_blank(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_whitespace())
}

fn is_xmlns(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

/// The newtype name [`whole_text`] asks for, which [`NodeDe`] answers with the
/// untrimmed text.
const WHOLE_TEXT: &str = "$io_webdav::whole_text";

/// Reads a property's text whole, untrimmed, for a `deserialize_with` field.
///
/// The PCDATA of `calendar-data` (RFC 4791 §9.6) and `address-data` (RFC 6352
/// §10.4) is the object itself, so its whitespace, the final CRLF included,
/// belongs to it. References are resolved and line ends normalised as for any
/// text.
pub(crate) fn whole_text<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    deserializer
        .deserialize_newtype_struct(WHOLE_TEXT, WholeTextVisitor)
        .map(Some)
}

struct WholeTextVisitor;

impl<'de> Visitor<'de> for WholeTextVisitor {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("text")
    }

    fn visit_str<E>(self, text: &str) -> Result<String, E> {
        Ok(text.to_string())
    }

    fn visit_string<E>(self, text: String) -> Result<String, E> {
        Ok(text)
    }
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
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        if name != WHOLE_TEXT {
            return visitor.visit_newtype_struct(self);
        }

        match self.node().take_whole_text() {
            Cow::Borrowed(text) => visitor.visit_borrowed_str(text),
            Cow::Owned(text) => visitor.visit_string(text),
        }
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
            seen: Seen::default(),
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
enum Value<'a> {
    None,
    Attr(&'a str),
    /// The `len` same-named siblings from node `first`.
    Group {
        first: u32,
        len: u32,
    },
}

/// An element's attributes, then its children grouped by name, as a map.
struct Children<'n, 'a> {
    nodes: &'n [Node<'a>],
    attrs: XmlAttrs<'a>,
    cur: u32,
    seen: Seen<'a>,
    value: Value<'a>,
}

/// The child names a [`Children`] map returned, the nodes of the first eight
/// inline.
#[derive(Default)]
struct Seen<'a> {
    inline: [u32; 8],
    len: usize,
    spill: BTreeSet<&'a str>,
}

impl<'a> Seen<'a> {
    /// Adds the name of node `i`, returning whether it was new.
    fn insert(&mut self, nodes: &[Node<'a>], i: u32) -> bool {
        let name = nodes[i as usize].local;
        let inline = &self.inline[..self.len];
        if inline
            .iter()
            .any(|&seen| same_name(nodes[seen as usize].local, name))
        {
            return false;
        }

        if self.len == self.inline.len() {
            return self.spill.insert(name);
        }

        self.inline[self.len] = i;
        self.len += 1;
        true
    }
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
            while self.cur != NONE && same_name(self.nodes[self.cur as usize].local, local) {
                self.cur = self.nodes[self.cur as usize].next;
                len += 1;
            }

            if !self.seen.insert(self.nodes, first) {
                continue;
            }

            self.value = Value::Group { first, len };
            return seed.deserialize(Key(local)).map(Some);
        }
    }

    fn next_value_seed<S: DeserializeSeed<'a>>(&mut self, seed: S) -> Result<S::Value, DeError> {
        match mem::replace(&mut self.value, Value::None) {
            Value::Attr(raw) => seed.deserialize(Attr(raw)),
            Value::Group { first, len } => {
                let first = NodeDe {
                    nodes: self.nodes,
                    i: first,
                };
                seed.deserialize(Group {
                    first,
                    len: len as usize,
                })
            }
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
        let body = ["<multistatus>", &"<a>".repeat(MULTISTATUS_MAX_DEPTH)].concat();
        let err = parse_multistatus_as::<Props>(&body).unwrap_err();
        assert_eq!(
            err,
            WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH)
        );
    }

    #[test]
    fn parse_multistatus_as_fails_on_a_body_that_is_not_a_whole_multistatus() {
        let html = "<html><head><title>Sign in</title></head><body>Please log in</body></html>";
        let cut = BODY.find("<d:response>\n  <d:href>/cal/gone.ics").unwrap() + 20;
        let open = BODY.strip_suffix("</d:multistatus>").unwrap();
        let twice = [BODY, "<d:response><d:href>/late</d:href></d:response>"].concat();
        let mismatched = "<d:multistatus><d:response><d:href>/b</d:x></d:multistatus>";

        for (body, err) in [
            (
                html,
                WebdavMultistatusParseError::UnexpectedRoot("html".into()),
            ),
            (&BODY[..cut], WebdavMultistatusParseError::Incomplete(2)),
            (open, WebdavMultistatusParseError::Incomplete(3)),
            (&twice, WebdavMultistatusParseError::Incomplete(3)),
            (mismatched, WebdavMultistatusParseError::Incomplete(0)),
            ("", WebdavMultistatusParseError::Incomplete(0)),
        ] {
            assert_eq!(parse_multistatus_as::<Props>(body).unwrap_err(), err);
        }
    }

    #[test]
    fn parse_multistatus_as_ignores_text_outside_the_root() {
        let body = ["\u{feff}notice\n", BODY, "\n<!-- served -->\n"].concat();
        let ms = parse_multistatus_as::<Props>(&body).unwrap();
        assert_eq!(ms.responses.len(), 3);
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
