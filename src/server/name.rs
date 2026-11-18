//! # Names
//!
//! Qualified XML names as a server reads them off a request body: the
//! namespace URI and the local name, prefixes resolved away.
//!
//! [`WebdavPropName`] types the properties of the DAV, CalDAV, CardDAV,
//! CalendarServer and Apple iCal vocabularies and keeps any other one by its
//! name; [`WebdavName`] names any other element (a resource type, a
//! component, a report).

use alloc::string::{String, ToString};

use crate::{
    rfc4791::calendar::{APPLE_ICAL, CALDAV},
    rfc4918::{CALENDARSERVER, DAV, WebdavProperty},
    rfc6352::addressbook::CARDDAV,
};

/// A qualified element name: namespace URI plus local name.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WebdavName {
    /// Namespace URI, empty for an element in no namespace.
    pub ns: String,
    /// Local name.
    pub local: String,
}

impl WebdavName {
    /// Whether this name is the one `element` names.
    pub fn is(&self, element: WebdavProperty) -> bool {
        self.ns == element.ns.uri && self.local == element.local
    }
}

impl From<WebdavProperty> for WebdavName {
    fn from(element: WebdavProperty) -> Self {
        Self {
            ns: element.ns.uri.to_string(),
            local: element.local.to_string(),
        }
    }
}

macro_rules! prop_names {
    ($($(#[$doc:meta])* $variant:ident => ($ns:expr, $local:literal),)*) => {
        /// A property name (RFC 4918 §4.3), typed for the vocabularies a
        /// CalDAV and CardDAV server serves, kept by name otherwise.
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum WebdavPropName {
            $($(#[$doc])* $variant,)*
            /// Any other property, a dead one included.
            Other(WebdavName),
        }

        impl WebdavPropName {
            /// Types the property `local` of namespace `ns`.
            pub fn new(ns: &str, local: &str) -> Self {
                $(if ns == $ns && local == $local {
                    return Self::$variant;
                })*

                Self::Other(WebdavName {
                    ns: ns.to_string(),
                    local: local.to_string(),
                })
            }

            /// The namespace URI.
            pub fn ns(&self) -> &str {
                match self {
                    $(Self::$variant => $ns,)*
                    Self::Other(name) => &name.ns,
                }
            }

            /// The local name.
            pub fn local(&self) -> &str {
                match self {
                    $(Self::$variant => $local,)*
                    Self::Other(name) => &name.local,
                }
            }
        }
    };
}

prop_names! {
    /// `DAV:creationdate` (RFC 4918 §15.1).
    CreationDate => (DAV.uri, "creationdate"),
    /// `DAV:displayname` (RFC 4918 §15.2).
    DisplayName => (DAV.uri, "displayname"),
    /// `DAV:getcontentlanguage` (RFC 4918 §15.3).
    GetContentLanguage => (DAV.uri, "getcontentlanguage"),
    /// `DAV:getcontentlength` (RFC 4918 §15.4).
    GetContentLength => (DAV.uri, "getcontentlength"),
    /// `DAV:getcontenttype` (RFC 4918 §15.5).
    GetContentType => (DAV.uri, "getcontenttype"),
    /// `DAV:getetag` (RFC 4918 §15.6).
    GetEtag => (DAV.uri, "getetag"),
    /// `DAV:getlastmodified` (RFC 4918 §15.7).
    GetLastModified => (DAV.uri, "getlastmodified"),
    /// `DAV:lockdiscovery` (RFC 4918 §15.8).
    LockDiscovery => (DAV.uri, "lockdiscovery"),
    /// `DAV:resourcetype` (RFC 4918 §15.9).
    ResourceType => (DAV.uri, "resourcetype"),
    /// `DAV:supportedlock` (RFC 4918 §15.10).
    SupportedLock => (DAV.uri, "supportedlock"),
    /// `DAV:supported-report-set` (RFC 3253 §3.1.5).
    SupportedReportSet => (DAV.uri, "supported-report-set"),
    /// `DAV:owner` (RFC 3744 §5.1).
    Owner => (DAV.uri, "owner"),
    /// `DAV:current-user-privilege-set` (RFC 3744 §5.4).
    CurrentUserPrivilegeSet => (DAV.uri, "current-user-privilege-set"),
    /// `DAV:principal-URL` (RFC 3744 §4.2).
    PrincipalUrl => (DAV.uri, "principal-URL"),
    /// `DAV:principal-collection-set` (RFC 3744 §5.8).
    PrincipalCollectionSet => (DAV.uri, "principal-collection-set"),
    /// `DAV:current-user-principal` (RFC 5397 §3).
    CurrentUserPrincipal => (DAV.uri, "current-user-principal"),
    /// `DAV:sync-token` (RFC 6578 §4).
    SyncToken => (DAV.uri, "sync-token"),
    /// `DAV:quota-available-bytes` (RFC 4331 §3).
    QuotaAvailableBytes => (DAV.uri, "quota-available-bytes"),
    /// `DAV:quota-used-bytes` (RFC 4331 §4).
    QuotaUsedBytes => (DAV.uri, "quota-used-bytes"),
    /// `C:calendar-home-set` (RFC 4791 §6.2.1).
    CalendarHomeSet => (CALDAV.uri, "calendar-home-set"),
    /// `C:calendar-description` (RFC 4791 §5.2.1).
    CalendarDescription => (CALDAV.uri, "calendar-description"),
    /// `C:calendar-timezone` (RFC 4791 §5.2.2).
    CalendarTimezone => (CALDAV.uri, "calendar-timezone"),
    /// `C:supported-calendar-component-set` (RFC 4791 §5.2.3).
    SupportedCalendarComponentSet => (CALDAV.uri, "supported-calendar-component-set"),
    /// `C:supported-calendar-data` (RFC 4791 §5.2.4).
    SupportedCalendarData => (CALDAV.uri, "supported-calendar-data"),
    /// `C:max-resource-size` (RFC 4791 §5.2.5).
    CalendarMaxResourceSize => (CALDAV.uri, "max-resource-size"),
    /// `C:min-date-time` (RFC 4791 §5.2.6).
    MinDateTime => (CALDAV.uri, "min-date-time"),
    /// `C:max-date-time` (RFC 4791 §5.2.7).
    MaxDateTime => (CALDAV.uri, "max-date-time"),
    /// `C:max-instances` (RFC 4791 §5.2.8).
    MaxInstances => (CALDAV.uri, "max-instances"),
    /// `C:max-attendees-per-instance` (RFC 4791 §5.2.9).
    MaxAttendeesPerInstance => (CALDAV.uri, "max-attendees-per-instance"),
    /// `C:calendar-data` (RFC 4791 §9.6).
    CalendarData => (CALDAV.uri, "calendar-data"),
    /// `C:calendar-user-address-set` (RFC 6638 §2.4.1).
    CalendarUserAddressSet => (CALDAV.uri, "calendar-user-address-set"),
    /// `CR:addressbook-home-set` (RFC 6352 §7.1.1).
    AddressbookHomeSet => (CARDDAV.uri, "addressbook-home-set"),
    /// `CR:principal-address` (RFC 6352 §7.1.2).
    PrincipalAddress => (CARDDAV.uri, "principal-address"),
    /// `CR:addressbook-description` (RFC 6352 §6.2.1).
    AddressbookDescription => (CARDDAV.uri, "addressbook-description"),
    /// `CR:supported-address-data` (RFC 6352 §6.2.2).
    SupportedAddressData => (CARDDAV.uri, "supported-address-data"),
    /// `CR:max-resource-size` (RFC 6352 §6.2.3).
    AddressbookMaxResourceSize => (CARDDAV.uri, "max-resource-size"),
    /// `CR:address-data` (RFC 6352 §10.4).
    AddressData => (CARDDAV.uri, "address-data"),
    /// `CS:getctag` (CalendarServer extension).
    Getctag => (CALENDARSERVER.uri, "getctag"),
    /// `A:calendar-color` (Apple iCal extension).
    CalendarColor => (APPLE_ICAL.uri, "calendar-color"),
    /// `A:calendar-order` (Apple iCal extension).
    CalendarOrder => (APPLE_ICAL.uri, "calendar-order"),
}

impl WebdavPropName {
    /// Whether this name is the one `prop` names.
    pub fn is(&self, prop: WebdavProperty) -> bool {
        self.ns() == prop.ns.uri && self.local() == prop.local
    }
}

impl From<WebdavProperty> for WebdavPropName {
    fn from(prop: WebdavProperty) -> Self {
        Self::new(prop.ns.uri, prop.local)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use crate::{
        rfc4791::calendar::CALENDAR_COLOR,
        rfc4918::{DISPLAYNAME, GETCTAG, WebdavNamespace, WebdavProperty},
        server::name::*,
    };

    #[test]
    fn known_names_are_typed_and_others_kept() {
        assert_eq!(
            WebdavPropName::from(DISPLAYNAME),
            WebdavPropName::DisplayName
        );
        assert_eq!(WebdavPropName::from(GETCTAG), WebdavPropName::Getctag);
        assert_eq!(
            WebdavPropName::from(CALENDAR_COLOR),
            WebdavPropName::CalendarColor
        );
        assert_eq!(
            WebdavPropName::new("urn:ietf:params:xml:ns:carddav", "max-resource-size"),
            WebdavPropName::AddressbookMaxResourceSize
        );

        let dead = WebdavPropName::new("urn:x", "displayname");
        let want = WebdavName {
            ns: "urn:x".to_string(),
            local: "displayname".to_string(),
        };
        assert_eq!(dead, WebdavPropName::Other(want));
        assert_eq!((dead.ns(), dead.local()), ("urn:x", "displayname"));

        let prop = WebdavProperty {
            ns: WebdavNamespace {
                uri: "urn:x",
                prefix: "X",
            },
            local: "displayname",
        };
        assert!(dead.is(prop));
        assert!(!dead.is(DISPLAYNAME));
    }
}
