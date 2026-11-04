//! # wddiff
//!
//! Compares the io-webdav working tree with a baseline snapshot (base.sh, a
//! git revision or the tree as it stood), on every mutated body, malformed
//! ones included: untyped parse, typed parses into seven property shapes,
//! errors and panics. Failing bodies land in target/.
//!
//! - `wddiff fuzz <iterations> <seed>`
//! - `wddiff show <file>`

mod fixture;

use std::{
    collections::BTreeMap,
    env, fs,
    panic::{AssertUnwindSafe, catch_unwind},
    process::exit,
};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
struct ItemProps {
    getetag: Option<String>,
    calendar_data: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RefProps<'a> {
    getetag: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
struct CalendarProps {
    resourcetype: Option<BTreeMap<String, ()>>,
    displayname: Option<String>,
    calendar_description: Option<String>,
    calendar_color: Option<String>,
    calendar_timezone: Option<String>,
    getctag: Option<String>,
    sync_token: Option<String>,
    supported_calendar_component_set: Option<CompSet>,
    supported_report_set: Option<ReportSet>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CompSet {
    #[serde(default)]
    comp: Vec<Comp>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Comp {
    #[serde(rename = "@name")]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
struct ReportSet {
    #[serde(default)]
    supported_report: Vec<SupportedReport>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SupportedReport {
    #[serde(default)]
    report: BTreeMap<String, ()>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
struct Mixed {
    getcontentlength: Option<u64>,
    enabled: Option<bool>,
    pair: Option<(String, Option<String>)>,
    kind: Option<Shape>,
    link: Option<Link>,
    x: Option<Vec<String>>,
    a: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum Shape {
    Circle,
    Square(String),
    Rect { w: u8, h: u8 },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Link {
    #[serde(rename = "@title")]
    title: Option<String>,
    #[serde(rename = "@name")]
    name: Option<String>,
}

macro_rules! outcome {
    ($e:expr) => {
        match catch_unwind(AssertUnwindSafe(|| format!("{:?}", $e))) {
            Ok(s) => s,
            Err(_) => "PANIC".to_string(),
        }
    };
}

fn both(xml: &str) -> Vec<(&'static str, String, String)> {
    use io_webdav::rfc4918::{parse_multistatus as new_untyped, typed::parse_multistatus_as as new};
    use io_webdav_base::rfc4918::{
        parse_multistatus as base_untyped, typed::parse_multistatus_as as base,
    };

    vec![
        ("untyped", outcome!(new_untyped(xml)), outcome!(base_untyped(xml))),
        ("item", outcome!(new::<ItemProps>(xml)), outcome!(base::<ItemProps>(xml))),
        ("ref", outcome!(new::<RefProps>(xml)), outcome!(base::<RefProps>(xml))),
        (
            "calendar",
            outcome!(new::<CalendarProps>(xml)),
            outcome!(base::<CalendarProps>(xml)),
        ),
        ("mixed", outcome!(new::<Mixed>(xml)), outcome!(base::<Mixed>(xml))),
        (
            "ignored",
            outcome!(new::<BTreeMap<String, serde::de::IgnoredAny>>(xml)),
            outcome!(base::<BTreeMap<String, serde::de::IgnoredAny>>(xml)),
        ),
        (
            "strs",
            outcome!(new::<BTreeMap<&str, &str>>(xml)),
            outcome!(base::<BTreeMap<&str, &str>>(xml)),
        ),
        (
            "strings",
            outcome!(new::<BTreeMap<String, String>>(xml)),
            outcome!(base::<BTreeMap<String, String>>(xml)),
        ),
    ]
}

const TOKENS: &[&str] = &[
    "<", ">", "</", "/>", "&", ";", "&amp;", "&lt;", "&gt;", "&quot;", "&apos;", "&#13;", "&#10;",
    "&#9;", "&#xD;", "&#xd;", "&#X41;", "&#x41;", "&#65;", "&#+65;", "&#x+41;", "&#0;", "&#;",
    "&#x;", "&#xD800;", "&#1114112;", "&#99999999999;", "&bogus;", "&am", "&#1", "\r", "\r\n",
    "\n", "\t", "\u{b}", "\u{c}", "\"", "'", "=", ":", " ", "  ", "\u{a0}", "\u{3000}", "\u{85}",
    "<!--", "-->", "<![CDATA[", "]]>", "<?", "?>", "<!DOCTYPE a>",
    "<!DOCTYPE a [<!ENTITY e \"x\">]>", "d:prop", "d:response", "d:propstat", "d:href",
    "d:supported-calendar-component-set", "averyveryverylongelementname", "a:b:c", ":", "::",
    " name=\"X\"", " name='Y&amp;'", " name=\"a>b\"", " title='x\"y'", " a=b", " xmlns:x=\"u\"",
    "é", "€", "😀", "<d:href>/h</d:href>", "<d:status>HTTP/1.1 404 Not Found</d:status>",
    "<d:status>HTTP/1.1 200 OK</d:status>", "<d:status> HTTP/1.1  207 x</d:status>",
    "<d:status>HTTP/1.1 +200 OK</d:status>", "<d:status>HTTP/1.1 99999 OK</d:status>",
    "<d:status>HTTP/1.1\u{a0}200 OK</d:status>", "<d:status>HTTP/1.1\u{b}200 OK</d:status>",
    "<d:prop>", "</d:prop>", "<d:x/>", "<d:x>", "</d:x>", "<d:x >", "</d:x >", "</d:x\u{a0}>",
    "<d:x\t/>", "<d:x/ >", "<x/>", "<a>", "</a>", "<a/>", "<b>t</b>", "<d:getetag>\"e\"</d:getetag>",
    "<d:enabled>true</d:enabled>", "<d:getcontentlength>12</d:getcontentlength>",
    "<d:kind><d:square>s</d:square></d:kind>", "<d:link title=\"t\" name='n'/>",
    "<d:x>a</d:x><d:x>b</d:x>", "<d:pair><d:a>1</d:a><d:b>2</d:b></d:pair>",
    "0123456789abcdef", "0123456789abcdef0123456789abcdef",
];

fn seeds() -> Vec<String> {
    let mut seeds: Vec<String> = vec![
        r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href> /a/b.ics </d:href><d:propstat><d:prop><d:getetag>"1"</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#.into(),
        r#"<multistatus xmlns="DAV:"><response><href>/c/</href><propstat><prop><resourcetype><collection/><C:calendar xmlns:C="urn:ietf:params:xml:ns:caldav"/></resourcetype><C:supported-calendar-component-set xmlns:C="urn:ietf:params:xml:ns:caldav"><C:comp name="VEVENT"/><C:comp name = 'VTO&amp;DO'/></C:supported-calendar-component-set></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#.into(),
        "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/x</d:href><d:propstat><d:prop><d:displayname>a&a&amp;b &#x41;&#65;&unknown; &lt;x&gt; c\r\nd\re &broken</d:displayname></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>".into(),
        "<?xml version=\"1.0\"?><!-- lead --><d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/y</d:href><d:propstat><d:prop><d:x>pre<![CDATA[<raw & \r\n>]]>post<!-- c --></d:x><d:y><d:z>deep</d:z>tail</d:y></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:propstat><d:prop><d:w/><d:v>no</d:v></d:prop><d:status>HTTP/1.1 403 Forbidden</d:status></d:propstat></d:response><d:sync-token> tok </d:sync-token></d:multistatus>".into(),
        "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/z</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response><d:response><d:href>/q</d:href><d:propstat><d:prop><d:a>1</d:b></d:prop></d:propstat></d:response></d:multistatus>".into(),
        "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href><d:x>/n</d:x></d:href><d:href>/second</d:href><d:propstat><d:prop><d:supported-report-set><d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report></d:supported-report-set><d:p><d:prop><d:inner>i</d:inner><d:e/></d:prop></d:p></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>".into(),
        "<d:multistatus xmlns:d=\"DAV:\">\r\n <d:response>\r\n  <d:href>/m</d:href>\r\n  <d:propstat>\r\n   <d:prop>\r\n    <d:getcontentlength> 12 </d:getcontentlength>\r\n    <d:enabled>1</d:enabled>\r\n    <d:pair><d:a>x</d:a><d:b>y</d:b></d:pair>\r\n    <d:kind><d:rect><d:w>2</d:w><d:h>3</d:h></d:rect></d:kind>\r\n    <d:link xmlns:x=\"urn:x\" title=\"a &amp; b\" x:name='n'/>\r\n    <d:x>one</d:x>\r\n    <d:x>two&#13;\r\nthree</d:x>\r\n    <d:a>&#x10FFFF;&#128512;\u{a0}</d:a>\r\n   </d:prop>\r\n   <d:status>HTTP/1.1 200 OK</d:status>\r\n  </d:propstat>\r\n </d:response>\r\n</d:multistatus>\r\n".into(),
    ];
    seeds.push(fixture::multiget(3, fixture::Eol::CrRef).xml);
    seeds.push(fixture::multiget(2, fixture::Eol::Crlf).xml);
    seeds.push(fixture::multiget(2, fixture::Eol::Lf).xml);
    seeds.push(fixture::sync(12).xml);
    seeds.push(fixture::propfind(3).xml);
    seeds
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fuzz") => fuzz(args[1].parse().unwrap(), args[2].parse().unwrap()),
        Some("show") => {
            let xml = fs::read_to_string(&args[1]).unwrap();
            for (name, new, base) in both(&xml) {
                if new != base {
                    println!("{name}:\n new:  {new}\n base: {base}");
                }
            }
        }
        _ => {
            eprintln!("usage: wddiff fuzz <n> <seed> | show <file>");
            exit(2);
        }
    }
}

fn fuzz(iterations: usize, seed: u64) {
    let seeds = seeds();
    std::panic::set_hook(Box::new(|_| {}));

    let mut rng = seed.max(1);
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let mut mismatches = 0;
    for i in 0..iterations {
        let mut s = match next() % 8 {
            // NOTE: token soup, for shapes no seed has.
            0 => {
                let mut s = String::new();
                for _ in 0..next() % 64 {
                    s.push_str(TOKENS[next() as usize % TOKENS.len()]);
                }
                s
            }
            _ => seeds[next() as usize % seeds.len()].clone(),
        };
        for _ in 0..1 + next() % 8 {
            let at = s.floor_char_boundary(next() as usize % (s.len() + 1));
            match next() % 6 {
                0 | 1 => s.insert_str(at, TOKENS[next() as usize % TOKENS.len()]),
                2 => {
                    let end = s.floor_char_boundary(at + next() as usize % 24);
                    s.replace_range(at..end.max(at), "");
                }
                3 => {
                    let end = s.floor_char_boundary(at + next() as usize % 64);
                    let chunk = s[at..end.max(at)].to_string();
                    let to = s.floor_char_boundary(next() as usize % (s.len() + 1));
                    s.insert_str(to, &chunk);
                }
                4 => {
                    // NOTE: a byte swapped for another of the markup ones.
                    let pick = [b'<', b'>', b'/', b'&', b';', b'\r', b'\n', b' ', b':', b'"'];
                    if at < s.len() && s.as_bytes()[at].is_ascii() {
                        let b = pick[next() as usize % pick.len()];
                        s.replace_range(at..at + 1, core::str::from_utf8(&[b]).unwrap());
                    }
                }
                _ => s.truncate(at),
            }
        }

        let diffs: Vec<_> = both(&s)
            .into_iter()
            .filter(|(_, new, base)| new != base)
            .collect();
        if !diffs.is_empty() {
            mismatches += 1;
            if mismatches <= 20 {
                fs::write(format!("target/fail-{seed}-{i}.xml"), &s).unwrap();
            }
        }
    }

    println!("seed {seed}: {iterations} bodies, {mismatches} mismatches");
    if mismatches > 0 {
        exit(1);
    }
}
