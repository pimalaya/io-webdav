//! # Generated bodies
//!
//! Multistatus bodies shaped like what CalDAV servers answer, indented as
//! sabre/dav and Radicale indent them:
//!
//! - `multiget-*`: a `calendar-multiget` REPORT, each response an event of
//!   about 1.5 KiB with its VTIMEZONE, line ends as LF, as literal CRLF, or as
//!   `&#13;` before LF (sabre/dav escapes CR);
//! - `sync`: a `sync-collection` REPORT, href and etag rows, one in ten a 404
//!   removal, closed by the sync token;
//! - `propfind`: a calendar list PROPFIND with every property listed, the
//!   home itself first and a 404 propstat on some.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Multiget,
    Sync,
    Propfind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Eol {
    Lf,
    Crlf,
    CrRef,
}

pub struct Fixture {
    pub name: String,
    pub kind: Kind,
    pub xml: String,
}

pub fn all() -> Vec<Fixture> {
    vec![
        multiget(1, Eol::CrRef),
        multiget(500, Eol::Lf),
        multiget(500, Eol::Crlf),
        multiget(500, Eol::CrRef),
        sync(5000),
        propfind(30),
    ]
}

const OPEN: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus xmlns:d=\"DAV:\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" xmlns:cs=\"http://calendarserver.org/ns/\" xmlns:x1=\"http://apple.com/ns/ical/\">\n";

pub fn multiget(n: usize, eol: Eol) -> Fixture {
    let mut xml = String::from(OPEN);
    for i in 0..n {
        let ics = event(i).replace(
            '\n',
            match eol {
                Eol::Lf => "\n",
                Eol::Crlf => "\r\n",
                Eol::CrRef => "&#13;\n",
            },
        );
        xml.push_str(&format!(
            " <d:response>\n  <d:href>/calendars/alice/personal/{i:08x}-4f1c-9a2e-{i:04}.ics</d:href>\n  <d:propstat>\n   <d:prop>\n    <d:getetag>\"{:016x}\"</d:getetag>\n    <cal:calendar-data>{ics}</cal:calendar-data>\n   </d:prop>\n   <d:status>HTTP/1.1 200 OK</d:status>\n  </d:propstat>\n </d:response>\n",
            (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ));
    }
    xml.push_str("</d:multistatus>\n");
    let eol = match eol {
        Eol::Lf => "lf",
        Eol::Crlf => "crlf",
        Eol::CrRef => "crref",
    };
    Fixture {
        name: format!("multiget{n}-{eol}"),
        kind: Kind::Multiget,
        xml,
    }
}

fn event(i: usize) -> String {
    format!(
        "BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Example Corp.//CalDAV Client//EN
CALSCALE:GREGORIAN
BEGIN:VTIMEZONE
TZID:Europe/Paris
BEGIN:DAYLIGHT
TZOFFSETFROM:+0100
TZOFFSETTO:+0200
TZNAME:CEST
DTSTART:19700329T020000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:+0200
TZOFFSETTO:+0100
TZNAME:CET
DTSTART:19701025T030000
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
UID:{i:08x}-4f1c-9a2e-{i:04}@example.com
DTSTAMP:20261104T091500Z
CREATED:20261001T080000Z
LAST-MODIFIED:20261103T171200Z
SEQUENCE:{}
DTSTART;TZID=Europe/Paris:20261110T{:02}0000
DTEND;TZID=Europe/Paris:20261110T{:02}3000
SUMMARY:R&amp;D sync #{i} with the platform team
LOCATION:Room 4.{}\\, building B
DESCRIPTION:Agenda: review the &lt;draft&gt; proposal\\, then walk through t
 he open questions from last week. Bring the numbers for Q4 and the list
 of blockers. Notes go to the shared document.
ORGANIZER;CN=Alice Martin:mailto:alice@example.com
ATTENDEE;CN=Bob;PARTSTAT=ACCEPTED;ROLE=REQ-PARTICIPANT:mailto:bob@example.com
ATTENDEE;CN=Carol;PARTSTAT=NEEDS-ACTION:mailto:carol@example.com
CATEGORIES:Work,Meeting
BEGIN:VALARM
ACTION:DISPLAY
DESCRIPTION:Reminder
TRIGGER:-PT15M
END:VALARM
END:VEVENT
END:VCALENDAR
",
        i % 7,
        8 + i % 9,
        8 + i % 9,
        i % 30
    )
}

pub fn sync(n: usize) -> Fixture {
    let mut xml = String::from(OPEN);
    for i in 0..n {
        if i % 10 == 9 {
            xml.push_str(&format!(
                " <d:response>\n  <d:href>/calendars/alice/personal/{i:08x}-4f1c-9a2e-{i:04}.ics</d:href>\n  <d:status>HTTP/1.1 404 Not Found</d:status>\n </d:response>\n"
            ));
        } else {
            xml.push_str(&format!(
                " <d:response>\n  <d:href>/calendars/alice/personal/{i:08x}-4f1c-9a2e-{i:04}.ics</d:href>\n  <d:propstat>\n   <d:prop>\n    <d:getetag>\"{:016x}\"</d:getetag>\n   </d:prop>\n   <d:status>HTTP/1.1 200 OK</d:status>\n  </d:propstat>\n </d:response>\n",
                (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ));
        }
    }
    xml.push_str(" <d:sync-token>http://sabre.io/ns/sync/48213</d:sync-token>\n</d:multistatus>\n");
    Fixture {
        name: format!("sync{n}"),
        kind: Kind::Sync,
        xml,
    }
}

pub fn propfind(n: usize) -> Fixture {
    let mut xml = String::from(OPEN);
    xml.push_str(" <d:response>\n  <d:href>/calendars/alice/</d:href>\n  <d:propstat>\n   <d:prop>\n    <d:resourcetype>\n     <d:collection/>\n    </d:resourcetype>\n    <d:displayname>alice</d:displayname>\n   </d:prop>\n   <d:status>HTTP/1.1 200 OK</d:status>\n  </d:propstat>\n </d:response>\n");
    for i in 0..n {
        let missing = if i % 3 == 0 {
            "  <d:propstat>\n   <d:prop>\n    <x1:calendar-color/>\n    <cal:calendar-timezone/>\n   </d:prop>\n   <d:status>HTTP/1.1 404 Not Found</d:status>\n  </d:propstat>\n"
        } else {
            ""
        };
        let color = if i % 3 == 0 {
            String::new()
        } else {
            format!(
                "    <x1:calendar-color>#{:06X}FF</x1:calendar-color>\n    <cal:calendar-timezone>BEGIN:VCALENDAR&#13;\nBEGIN:VTIMEZONE&#13;\nTZID:Europe/Paris&#13;\nEND:VTIMEZONE&#13;\nEND:VCALENDAR&#13;\n</cal:calendar-timezone>\n",
                i * 0x123457 % 0xFFFFFF
            )
        };
        xml.push_str(&format!(
            " <d:response>\n  <d:href>/calendars/alice/cal-{i}/</d:href>\n  <d:propstat>\n   <d:prop>\n    <d:resourcetype>\n     <d:collection/>\n     <cal:calendar/>\n    </d:resourcetype>\n    <d:displayname>Calendar {i} &amp; friends</d:displayname>\n    <cal:calendar-description>Shared with the team</cal:calendar-description>\n{color}    <cal:supported-calendar-component-set>\n     <cal:comp name=\"VEVENT\"/>\n     <cal:comp name=\"VTODO\"/>\n    </cal:supported-calendar-component-set>\n    <cs:getctag>http://sabre.io/ns/sync/{i}</cs:getctag>\n    <d:sync-token>http://sabre.io/ns/sync/{i}</d:sync-token>\n    <d:supported-report-set>\n     <d:supported-report>\n      <d:report>\n       <d:expand-property/>\n      </d:report>\n     </d:supported-report>\n     <d:supported-report>\n      <d:report>\n       <d:sync-collection/>\n      </d:report>\n     </d:supported-report>\n     <d:supported-report>\n      <d:report>\n       <cal:calendar-multiget/>\n      </d:report>\n     </d:supported-report>\n    </d:supported-report-set>\n   </d:prop>\n   <d:status>HTTP/1.1 200 OK</d:status>\n  </d:propstat>\n{missing} </d:response>\n"
        ));
    }
    xml.push_str("</d:multistatus>\n");
    Fixture {
        name: format!("propfind{n}"),
        kind: Kind::Propfind,
        xml,
    }
}
