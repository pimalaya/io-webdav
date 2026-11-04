//! # wdxml
//!
//! Compares io-webdav's multistatus parsing before the change (39bb548,
//! xmlparser, extracted by old.sh) with the current crate (scanner, typed
//! properties).
//!
//! - `wdxml check`: outputs agree on the fixtures and edge cases.
//! - `wdxml fuzz <iterations> <seed>`: mutated bodies; the new parsers never
//!   panic, and the untyped one agrees with the old one on every body
//!   xmlparser tokenizes without error. Failing bodies land in target/fuzz/.
//! - `wdxml time [rounds]`: in-process timing, best round per row.
//! - `wdxml run <variant> <fixture> <iterations>`: one row, for `perf stat`.

mod consumer;
mod fixture;

use std::{
    env, fs,
    hint::black_box,
    panic::{AssertUnwindSafe, catch_unwind},
    process::exit,
    time::Instant,
};

use crate::{
    consumer::Out,
    fixture::{Fixture, Kind},
};

const VARIANTS: [&str; 5] = ["pre", "parse-old", "parse-new", "map-old", "map-new"];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check(),
        Some("fuzz") => fuzz(args[1].parse().unwrap(), args[2].parse().unwrap()),
        Some("show") => {
            let xml = fs::read_to_string(&args[1]).unwrap();
            println!("old: {}\nnew: {}", old_parse(&xml), new_parse(&xml));
        }
        Some("time") => time(args.get(1).map_or(15, |n| n.parse().unwrap())),
        Some("run") => {
            let fixtures = fixture::all();
            let f = fixtures
                .iter()
                .find(|f| f.name == args[2])
                .expect("fixture");
            let n: usize = args[3].parse().unwrap();
            let body = f.xml.clone().into_bytes();
            for _ in 0..n {
                run(&args[1], f.kind, black_box(&body));
            }
        }
        _ => {
            eprintln!(
                "usage: wdxml check | fuzz <n> <seed> | time [rounds] | run <variant> <fixture> <n>"
            );
            exit(2);
        }
    }
}

/// Runs one variant on a response body, as the coroutines receive it.
#[inline(never)]
fn run(variant: &str, kind: Kind, body: &[u8]) {
    let xml = || core::str::from_utf8(body).unwrap();
    match variant {
        "pre" => {
            let copy = black_box(body.to_vec());
            black_box(core::str::from_utf8(&copy).is_ok());
        }
        "parse-old" => drop(black_box(io_webdav_old::rfc4918::parse_multistatus(xml()))),
        "parse-new" => drop(black_box(io_webdav::rfc4918::parse_multistatus(xml()))),
        "map-old" => drop(black_box(old_map(kind, xml()))),
        "map-new" => drop(black_box(new_map(kind, xml()))),
        _ => panic!("unknown variant {variant}"),
    }
}

fn old_map(kind: Kind, xml: &str) -> Out {
    let ms = io_webdav_old::rfc4918::parse_multistatus(xml).unwrap();
    match kind {
        Kind::Multiget => consumer::old_items(ms),
        Kind::Sync => consumer::old_refs(ms),
        Kind::Propfind => consumer::old_calendars(ms),
    }
}

fn new_map(kind: Kind, xml: &str) -> Out {
    match kind {
        Kind::Multiget => consumer::new_items(xml),
        Kind::Sync => consumer::new_refs(xml),
        Kind::Propfind => consumer::new_calendars(xml),
    }
}

fn old_parse(xml: &str) -> String {
    format!("{:?}", io_webdav_old::rfc4918::parse_multistatus(xml))
}

fn new_parse(xml: &str) -> String {
    format!("{:?}", io_webdav::rfc4918::parse_multistatus(xml))
}

fn check() {
    let mut failed = false;
    for f in fixture::all() {
        if old_parse(&f.xml) != new_parse(&f.xml) {
            failed = true;
            eprintln!("{}: parse differs", f.name);
        }
        let (old, new) = (old_map(f.kind, &f.xml), new_map(f.kind, &f.xml));
        if old != new {
            failed = true;
            eprintln!("{}: map differs", f.name);
        }
        println!("{:<20} {:>9} bytes agree", f.name, f.xml.len());
    }

    for (i, xml) in EDGES.iter().enumerate() {
        let (old, new) = (old_parse(xml), new_parse(xml));
        if old != new {
            failed = true;
            eprintln!("edge {i}: parse differs\n old: {old}\n new: {new}");
        }
    }
    println!("{} edge cases agree", EDGES.len());

    let deep = "<a>".repeat(100);
    if old_parse(&deep) != new_parse(&deep) {
        failed = true;
        eprintln!("depth cap differs");
    }

    if failed {
        exit(1);
    }
}

const TOKENS: &[&str] = &[
    "<",
    ">",
    "</",
    "/>",
    "&",
    ";",
    "&amp;",
    "&#13;",
    "&#x41;",
    "&bogus;",
    "\r",
    "\r\n",
    "\n",
    "\"",
    "'",
    "=",
    ":",
    " ",
    "<!--",
    "-->",
    "<![CDATA[",
    "]]>",
    "<?",
    "?>",
    "<!DOCTYPE a>",
    "<!DOCTYPE a [<!ENTITY e \"x\">]>",
    "d:prop",
    "d:response",
    "d:propstat",
    "d:href",
    " name=\"X\"",
    " name='Y&amp;'",
    "é",
    "€",
    "<d:href>/h</d:href>",
    "<d:status>HTTP/1.1 404 Not Found</d:status>",
    "<d:prop>",
    "</d:prop>",
    "<d:x/>",
];

fn fuzz(iterations: usize, seed: u64) {
    let fixtures = fixture::all();
    let mut seeds: Vec<String> = EDGES.iter().map(|s| s.to_string()).collect();
    seeds.push(fixture::multiget(3, fixture::Eol::CrRef).xml);
    seeds.push(fixture::multiget(2, fixture::Eol::Crlf).xml);
    seeds.push(fixture::sync(12).xml);
    seeds.push(fixture::propfind(3).xml);
    seeds.extend(
        fixtures
            .iter()
            .filter(|f| f.xml.len() < 4096)
            .map(|f| f.xml.clone()),
    );

    fs::create_dir_all("target/fuzz").unwrap();
    std::panic::set_hook(Box::new(|_| {}));

    let mut rng = seed.max(1);
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let (mut accepted, mut mismatches, mut panics) = (0, 0, 0);
    for i in 0..iterations {
        let mut s = seeds[next() as usize % seeds.len()].clone();
        for _ in 0..1 + next() % 8 {
            let at = s.floor_char_boundary(next() as usize % (s.len() + 1));
            match next() % 5 {
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
                _ => s.truncate(at),
            }
        }

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let new = new_parse(&s);
            consumer::fuzz_typed(&s);
            new
        }));
        let Ok(new) = outcome else {
            panics += 1;
            fs::write(format!("target/fuzz/panic-{seed}-{i}.xml"), &s).unwrap();
            continue;
        };

        if xmlparser::Tokenizer::from(s.as_str()).all(|token| token.is_ok()) {
            accepted += 1;
            if old_parse(&s) != new {
                mismatches += 1;
                if mismatches <= 20 {
                    fs::write(format!("target/fuzz/mismatch-{seed}-{i}.xml"), &s).unwrap();
                }
            }
        }
    }

    println!(
        "seed {seed}: {iterations} bodies, {accepted} tokenized by xmlparser, {mismatches} mismatches, {panics} panics"
    );
    if mismatches > 0 || panics > 0 {
        exit(1);
    }
}

fn time(rounds: usize) {
    let fixtures = fixture::all();
    print!("{:<20} {:>9}", "fixture", "bytes");
    for v in VARIANTS {
        print!(" {v:>12}");
    }
    println!(
        "\n{:<20} {:>9} (µs per body, best of {rounds} rounds; MB/s below)",
        "", ""
    );
    for f in &fixtures {
        let body = f.xml.clone().into_bytes();
        let best: Vec<f64> = VARIANTS
            .iter()
            .map(|v| measure(v, f, &body, rounds))
            .collect();
        print!("{:<20} {:>9}", f.name, body.len());
        for t in &best {
            print!(" {:>12.1}", t * 1e6);
        }
        print!("\n{:<20} {:>9}", "", "");
        for t in &best {
            print!(" {:>12.0}", body.len() as f64 / t / 1e6);
        }
        println!();
    }
}

fn measure(variant: &str, f: &Fixture, body: &[u8], rounds: usize) -> f64 {
    let start = Instant::now();
    run(variant, f.kind, body);
    let once = start.elapsed().as_secs_f64().max(1e-7);
    let n = ((0.02 / once) as usize).clamp(1, 100_000);
    (0..rounds)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..n {
                run(variant, f.kind, black_box(body));
            }
            start.elapsed().as_secs_f64() / n as f64
        })
        .fold(f64::INFINITY, f64::min)
}

const EDGES: &[&str] = &[
    r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href> /a/b.ics </d:href><d:propstat><d:prop><d:getetag>"1"</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#,
    r#"<multistatus xmlns="DAV:"><response><href>/c/</href><propstat><prop><resourcetype><collection/><C:calendar xmlns:C="urn:ietf:params:xml:ns:caldav"/></resourcetype><C:supported-calendar-component-set xmlns:C="urn:ietf:params:xml:ns:caldav"><C:comp name="VEVENT"/><C:comp name = 'VTO&amp;DO'/></C:supported-calendar-component-set></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#,
    "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/x</d:href><d:propstat><d:prop><d:displayname>a&a&amp;b &#x41;&#65;&unknown; &lt;x&gt; c\r\nd\re &broken</d:displayname></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>",
    "<?xml version=\"1.0\"?><!-- lead --><d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/y</d:href><d:propstat><d:prop><d:x>pre<![CDATA[<raw & \r\n>]]>post<!-- c --></d:x><d:y><d:z>deep</d:z>tail</d:y></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:propstat><d:prop><d:w/><d:v>no</d:v></d:prop><d:status>HTTP/1.1 403 Forbidden</d:status></d:propstat></d:response><d:sync-token> tok </d:sync-token></d:multistatus>",
    "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/z</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response><d:response><d:href>/q</d:href><d:propstat><d:prop><d:a>1</d:b></d:prop></d:propstat></d:response></d:multistatus>",
    "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/t</d:href><d:propstat><d:prop><d:a>trunc",
    "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href><d:x>/n</d:x></d:href><d:href>/second</d:href><d:propstat><d:prop><d:supported-report-set><d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report></d:supported-report-set><d:p><d:prop><d:inner>i</d:inner><d:e/></d:prop></d:p></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>",
];
