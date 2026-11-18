//! # Request headers
//!
//! Parsers for the request headers WebDAV defines (RFC 4918 §10) plus the
//! entity-tag preconditions of RFC 9110 §13.1: `Depth`, `Destination`,
//! `Overwrite`, `If`, `If-Match` and `If-None-Match`, `Timeout` and
//! `Lock-Token`. Each takes the header value and answers [`None`] when it is
//! malformed, which a server answers with 400.

use core::{fmt, str};

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::rfc4918::decoded;

/// The `Depth` header (RFC 4918 §10.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebdavDepth {
    /// `0`: the resource alone.
    Zero,
    /// `1`: the resource and its direct members.
    One,
    /// `infinity`: the resource and every member, at any depth.
    Infinity,
}

/// The `Destination` header (RFC 4918 §10.3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavDestination {
    /// The `scheme://authority` of an absolute URL, [`None`] for a path.
    pub origin: Option<String>,
    /// The path, percent-decoded, query and fragment dropped.
    pub path: String,
}

/// The `If` header (RFC 4918 §10.4): the request applies when any list
/// matches.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavIf {
    /// The lists, in order.
    pub lists: Vec<WebdavIfList>,
}

/// One list of an `If` header: matches when every condition does.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavIfList {
    /// The resource tag of a tagged list, [`None`] for the request URI.
    pub resource: Option<String>,
    /// The conditions.
    pub conditions: Vec<WebdavIfCondition>,
}

/// One condition of an `If` list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavIfCondition {
    /// Whether the condition is negated with `Not`.
    pub not: bool,
    /// What the resource is tested against.
    pub test: WebdavIfTest,
}

/// What an `If` condition tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavIfTest {
    /// A state token, such as a lock token, without its angle brackets.
    Token(String),
    /// An entity tag.
    Etag(WebdavEntityTag),
}

/// An entity tag (RFC 9110 §8.8.3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavEntityTag {
    /// Whether the tag is weak (`W/`).
    pub weak: bool,
    /// The opaque value, without its quotes.
    pub opaque: String,
}

/// The `If-Match` or `If-None-Match` header (RFC 9110 §13.1.1, §13.1.2).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavEtagMatch {
    /// `*`: any current representation.
    Any,
    /// The listed entity tags.
    Tags(Vec<WebdavEntityTag>),
}

/// One value of the `Timeout` header (RFC 4918 §10.7).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebdavTimeout {
    /// `Second-n`.
    Seconds(u32),
    /// `Infinite`.
    Infinite,
}

impl WebdavDepth {
    /// The header value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::One => "1",
            Self::Infinity => "infinity",
        }
    }
}

impl WebdavEntityTag {
    /// Whether two tags match by the strong comparison of RFC 9110 §8.8.3.2.
    pub fn strong_eq(&self, other: &Self) -> bool {
        !self.weak && !other.weak && self.opaque == other.opaque
    }

    /// Whether two tags match by the weak comparison of RFC 9110 §8.8.3.2.
    pub fn weak_eq(&self, other: &Self) -> bool {
        self.opaque == other.opaque
    }
}

impl fmt::Display for WebdavEntityTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.weak {
            true => write!(f, "W/\"{}\"", self.opaque),
            false => write!(f, "\"{}\"", self.opaque),
        }
    }
}

impl fmt::Display for WebdavTimeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Seconds(seconds) => write!(f, "Second-{seconds}"),
            Self::Infinite => f.write_str("Infinite"),
        }
    }
}

/// Parses a `Depth` header value, case-insensitively.
pub fn parse_depth(value: &str) -> Option<WebdavDepth> {
    match value.trim() {
        "0" => Some(WebdavDepth::Zero),
        "1" => Some(WebdavDepth::One),
        value if value.eq_ignore_ascii_case("infinity") => Some(WebdavDepth::Infinity),
        _ => None,
    }
}

/// Parses a `Destination` header value: an absolute URL or an absolute path.
///
/// The path comes back percent-decoded and is refused when the decoding is
/// not UTF-8; resolving its dot segments is left to the server.
pub fn parse_destination(value: &str) -> Option<WebdavDestination> {
    let value = value.trim();
    let value = value.split(['?', '#']).next().unwrap_or(value);

    let (origin, path) = match value.split_once("://") {
        Some((scheme, rest)) => {
            let valid = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
            if !valid {
                return None;
            }

            let split = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(split);
            if authority.is_empty() {
                return None;
            }

            let origin = value[..scheme.len() + 3 + authority.len()].to_string();
            (Some(origin), if path.is_empty() { "/" } else { path })
        }
        None if value.starts_with('/') => (None, value),
        None => return None,
    };

    let path = String::from_utf8(decoded(path).collect()).ok()?;
    Some(WebdavDestination { origin, path })
}

/// Parses an `Overwrite` header value, `T` or `F`.
pub fn parse_overwrite(value: &str) -> Option<bool> {
    match value.trim() {
        "T" | "t" => Some(true),
        "F" | "f" => Some(false),
        _ => None,
    }
}

/// Parses a `Lock-Token` header value, a coded URL, into the token without
/// its angle brackets.
pub fn parse_lock_token(value: &str) -> Option<String> {
    let token = value.trim().strip_prefix('<')?.strip_suffix('>')?.trim();
    (!token.is_empty() && !token.contains(['<', '>'])).then(|| token.to_string())
}

/// Parses a `Timeout` header value into its values, in order of preference.
///
/// A number of seconds past `u32::MAX` saturates.
pub fn parse_timeout(value: &str) -> Option<Vec<WebdavTimeout>> {
    let mut timeouts = Vec::new();

    for part in value.split(',') {
        let part = part.trim();

        if part.eq_ignore_ascii_case("infinite") {
            timeouts.push(WebdavTimeout::Infinite);
            continue;
        }

        let (prefix, seconds) = part.split_at_checked(7)?;
        if !prefix.eq_ignore_ascii_case("second-")
            || seconds.is_empty()
            || !seconds.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }

        let seconds = seconds.parse().unwrap_or(u32::MAX);
        timeouts.push(WebdavTimeout::Seconds(seconds));
    }

    (!timeouts.is_empty()).then_some(timeouts)
}

/// Parses an `If-Match` or `If-None-Match` header value.
pub fn parse_etag_match(value: &str) -> Option<WebdavEtagMatch> {
    let value = value.trim();
    if value == "*" {
        return Some(WebdavEtagMatch::Any);
    }

    let mut tags = Vec::new();
    let mut rest = value;

    loop {
        rest = rest.trim_start_matches([' ', '\t', ',']);
        if rest.is_empty() {
            break;
        }

        let (tag, tail) = entity_tag(rest)?;
        tags.push(tag);
        rest = tail;

        let after = rest.trim_start_matches([' ', '\t']);
        if !after.is_empty() && !after.starts_with(',') {
            return None;
        }
    }

    (!tags.is_empty()).then_some(WebdavEtagMatch::Tags(tags))
}

/// Parses an `If` header value.
///
/// Lists are all tagged or all untagged, as RFC 4918 §10.4.2 has them.
pub fn parse_if(value: &str) -> Option<WebdavIf> {
    let mut lists = Vec::new();
    let mut resource: Option<String> = None;
    let mut tagged: Option<bool> = None;
    let mut rest = value;

    loop {
        rest = rest.trim_start();

        let Some(first) = rest.chars().next() else {
            break;
        };

        match first {
            '<' => {
                if tagged == Some(false) {
                    return None;
                }

                let end = rest.find('>')?;
                resource = Some(rest[1..end].to_string());
                tagged = Some(true);
                rest = &rest[end + 1..];

                if !rest.trim_start().starts_with('(') {
                    return None;
                }
            }
            '(' => {
                if tagged.is_none() {
                    tagged = Some(false);
                }

                let (conditions, tail) = if_list(&rest[1..])?;
                lists.push(WebdavIfList {
                    resource: resource.clone(),
                    conditions,
                });
                rest = tail;
            }
            _ => return None,
        }
    }

    (!lists.is_empty()).then_some(WebdavIf { lists })
}

/// Reads the conditions of an `If` list up to its closing parenthesis.
fn if_list(mut rest: &str) -> Option<(Vec<WebdavIfCondition>, &str)> {
    let mut conditions = Vec::new();

    loop {
        rest = rest.trim_start();

        if let Some(tail) = rest.strip_prefix(')') {
            return (!conditions.is_empty()).then_some((conditions, tail));
        }

        let not = match rest.get(..3) {
            Some(word) if word.eq_ignore_ascii_case("not") => {
                rest = rest[3..].trim_start();
                true
            }
            _ => false,
        };

        let test = match rest.chars().next()? {
            '<' => {
                let end = rest.find('>')?;
                let token = &rest[1..end];
                rest = &rest[end + 1..];
                WebdavIfTest::Token(token.to_string())
            }
            '[' => {
                let (tag, tail) = entity_tag(&rest[1..])?;
                rest = tail.trim_start().strip_prefix(']')?;
                WebdavIfTest::Etag(tag)
            }
            _ => return None,
        };

        conditions.push(WebdavIfCondition { not, test });
    }
}

/// Reads one entity tag at the start of `rest`.
fn entity_tag(rest: &str) -> Option<(WebdavEntityTag, &str)> {
    let (weak, rest) = match rest.strip_prefix("W/") {
        Some(rest) => (true, rest),
        None => (false, rest),
    };

    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    let opaque = rest[..end].to_string();

    Some((WebdavEntityTag { weak, opaque }, &rest[end + 1..]))
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec};

    use crate::server::header::*;

    fn tag(weak: bool, opaque: &str) -> WebdavEntityTag {
        WebdavEntityTag {
            weak,
            opaque: opaque.to_string(),
        }
    }

    #[test]
    fn depth_reads_the_three_values() {
        assert_eq!(parse_depth("0"), Some(WebdavDepth::Zero));
        assert_eq!(parse_depth(" 1 "), Some(WebdavDepth::One));
        assert_eq!(parse_depth("Infinity"), Some(WebdavDepth::Infinity));
        assert_eq!(parse_depth("2"), None);
    }

    #[test]
    fn destination_reads_urls_and_paths_decoded() {
        let destination = parse_destination("https://dav.example.org:8443/a%20b/c%C3%A9?q#f");
        assert_eq!(
            destination,
            Some(WebdavDestination {
                origin: Some("https://dav.example.org:8443".to_string()),
                path: "/a b/cé".to_string(),
            })
        );

        let path = parse_destination("/cal/x%2Fy.ics").unwrap();
        assert_eq!((path.origin, path.path.as_str()), (None, "/cal/x/y.ics"));
        assert_eq!(parse_destination("http://host").unwrap().path, "/");
        assert_eq!(parse_destination("relative/path"), None);
        assert_eq!(parse_destination("http:///path"), None);
        assert_eq!(parse_destination("/%FF"), None);
    }

    #[test]
    fn overwrite_and_lock_token_read_their_forms() {
        assert_eq!(parse_overwrite("T"), Some(true));
        assert_eq!(parse_overwrite("F"), Some(false));
        assert_eq!(parse_overwrite("yes"), None);

        let token = parse_lock_token(" <urn:uuid:e71d4fae-5dec> ");
        assert_eq!(token.as_deref(), Some("urn:uuid:e71d4fae-5dec"));
        assert_eq!(parse_lock_token("urn:uuid:x"), None);
        assert_eq!(parse_lock_token("<>"), None);
    }

    #[test]
    fn timeout_reads_a_preference_list() {
        assert_eq!(
            parse_timeout("Infinite, Second-4100000000"),
            Some(vec![
                WebdavTimeout::Infinite,
                WebdavTimeout::Seconds(4100000000)
            ])
        );
        assert_eq!(
            parse_timeout("Second-99999999999"),
            Some(vec![WebdavTimeout::Seconds(u32::MAX)])
        );
        assert_eq!(parse_timeout("Second-"), None);
        assert_eq!(parse_timeout("Minute-5"), None);
        assert_eq!(WebdavTimeout::Seconds(5).to_string(), "Second-5");
    }

    #[test]
    fn etag_match_reads_star_and_lists() {
        assert_eq!(parse_etag_match("*"), Some(WebdavEtagMatch::Any));
        assert_eq!(
            parse_etag_match("\"a,b\", W/\"c\""),
            Some(WebdavEtagMatch::Tags(vec![
                tag(false, "a,b"),
                tag(true, "c")
            ]))
        );
        assert_eq!(parse_etag_match("\"a\" \"b\""), None);
        assert_eq!(parse_etag_match("a"), None);
        assert_eq!(parse_etag_match(""), None);
        assert_eq!(tag(true, "c").to_string(), "W/\"c\"");
        assert!(tag(false, "a").strong_eq(&tag(false, "a")));
        assert!(!tag(true, "a").strong_eq(&tag(false, "a")));
        assert!(tag(true, "a").weak_eq(&tag(false, "a")));
    }

    #[test]
    fn if_reads_the_rfc_4918_examples() {
        let value = "(<urn:uuid:181d4fae-7d8c-11d0-a765-00a0c91e6bf2> \
                     [\"I am an ETag\"]) ([\"I am another ETag\"])";
        let header = parse_if(value).unwrap();
        assert_eq!(header.lists.len(), 2);
        assert_eq!(header.lists[0].resource, None);
        assert_eq!(
            header.lists[0].conditions,
            [
                WebdavIfCondition {
                    not: false,
                    test: WebdavIfTest::Token(
                        "urn:uuid:181d4fae-7d8c-11d0-a765-00a0c91e6bf2".to_string()
                    ),
                },
                WebdavIfCondition {
                    not: false,
                    test: WebdavIfTest::Etag(tag(false, "I am an ETag")),
                },
            ]
        );

        let value = "(Not <urn:uuid:181d4fae-7d8c-11d0-a765-00a0c91e6bf2> \
                     <urn:uuid:58f202ac-22cf-11d1-b12d-002035b29092>)";
        let header = parse_if(value).unwrap();
        assert!(header.lists[0].conditions[0].not);
        assert!(!header.lists[0].conditions[1].not);

        let value = "<http://www.example.com/specs/> \
                     (<urn:uuid:181d4fae-7d8c-11d0-a765-00a0c91e6bf2>) \
                     <http://www.example.com/a> ([W/\"x\"]) (Not [\"y\"])";
        let header = parse_if(value).unwrap();
        let resources: Vec<_> = header
            .lists
            .iter()
            .map(|list| list.resource.as_deref())
            .collect();
        assert_eq!(
            resources,
            [
                Some("http://www.example.com/specs/"),
                Some("http://www.example.com/a"),
                Some("http://www.example.com/a"),
            ]
        );
    }

    #[test]
    fn if_refuses_malformed_values() {
        assert_eq!(parse_if(""), None);
        assert_eq!(parse_if("()"), None);
        assert_eq!(parse_if("(<a>) <http://x/> (<b>)"), None);
        assert_eq!(parse_if("<http://x/>"), None);
        assert_eq!(parse_if("(<a>"), None);
        assert_eq!(parse_if("([\"a\")"), None);
        assert_eq!(parse_if("(token)"), None);
    }
}
