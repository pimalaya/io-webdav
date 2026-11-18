#![no_main]

//! Arbitrary bytes never panic a request body or header parser, every raw
//! element a parser keeps stands alone (wrapped in a fresh body it reads back
//! to the same name, text and children), and a multistatus written out of it
//! reads back through the client parser.

use io_webdav::{
    rfc4918::parse_multistatus,
    server::{
        body::WebdavPropElement,
        header::{
            parse_depth, parse_destination, parse_etag_match, parse_if, parse_lock_token,
            parse_overwrite, parse_timeout,
        },
        lock::parse_lockinfo,
        mkcol::{parse_mkcalendar, parse_mkcol},
        multistatus::{
            WebdavMultistatusBody, WebdavPropBody, WebdavPropContent, WebdavResponseBody,
        },
        propfind::parse_propfind,
        proppatch::{WebdavPropUpdate, parse_proppatch},
        report::parse_report,
    },
};
use libfuzzer_sys::fuzz_target;

fn stands_alone(element: &WebdavPropElement) {
    let body = format!(
        "<D:propertyupdate xmlns:D=\"DAV:\"><D:set><D:prop>{}</D:prop></D:set></D:propertyupdate>",
        element.xml
    );
    let updates = parse_proppatch(body.as_bytes()).expect("raw element reparses");
    let [WebdavPropUpdate::Set(again)] = updates.as_slice() else {
        panic!("raw element reparses to {updates:?}");
    };
    assert_eq!(again.name, element.name);
    assert_eq!(again.text, element.text);
    assert_eq!(again.children, element.children);
}

fn reads_back(elements: &[WebdavPropElement]) {
    let mut response = WebdavResponseBody {
        href: "/r".into(),
        ..Default::default()
    };
    for element in elements {
        let content = WebdavPropContent::Element(element.xml.clone());
        let name = element.name.clone();
        response.push(200, WebdavPropBody { name, content });
        let text = WebdavPropContent::Text(element.text.clone());
        let name = element.name.clone();
        response.push(
            404,
            WebdavPropBody {
                name,
                content: text,
            },
        );
    }

    let body = WebdavMultistatusBody {
        responses: vec![response],
        ..Default::default()
    };
    let ms = parse_multistatus(&body.to_xml()).expect("written multistatus reads back");
    assert_eq!(ms.responses[0].props.len(), elements.len());
    assert_eq!(ms.responses[0].failures.len(), elements.len());
}

fuzz_target!(|data: &[u8]| {
    let _ = parse_propfind(data);
    let _ = parse_report(data);
    let _ = parse_lockinfo(data);

    for parsed in [parse_mkcol(data), parse_mkcalendar(data)] {
        if let Ok(elements) = parsed {
            elements.iter().for_each(stands_alone);
            reads_back(&elements);
        }
    }

    if let Ok(updates) = parse_proppatch(data) {
        let elements: Vec<_> = updates
            .into_iter()
            .filter_map(|update| match update {
                WebdavPropUpdate::Set(element) => Some(element),
                WebdavPropUpdate::Remove(_) => None,
            })
            .collect();
        elements.iter().for_each(stands_alone);
        reads_back(&elements);
    }

    if let Ok(value) = std::str::from_utf8(data) {
        let _ = parse_depth(value);
        let _ = parse_destination(value);
        let _ = parse_overwrite(value);
        let _ = parse_if(value);
        let _ = parse_etag_match(value);
        let _ = parse_timeout(value);
        let _ = parse_lock_token(value);
    }
});
