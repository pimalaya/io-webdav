//! # Preconditions
//!
//! The precondition and postcondition codes a server reports in a
//! `DAV:error` body (RFC 4918 §16), for the WebDAV core and the CalDAV,
//! CardDAV, collection synchronization and versioning extensions.

use core::slice;

use alloc::{format, string::String, vec::Vec};

use crate::{
    rfc4918::{XML_DECL, escape_text},
    server::multistatus::NAMESPACE_DECLS,
};

/// A failed precondition or postcondition, the element a `DAV:error` holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavPrecondition {
    /// `DAV:lock-token-matches-request-uri` (RFC 4918 §16).
    LockTokenMatchesRequestUri,
    /// `DAV:lock-token-submitted`, with the hrefs of the locked resources
    /// (RFC 4918 §16).
    LockTokenSubmitted(Vec<String>),
    /// `DAV:no-conflicting-lock`, with the hrefs of the conflicting lock
    /// roots (RFC 4918 §16).
    NoConflictingLock(Vec<String>),
    /// `DAV:no-external-entities` (RFC 4918 §16).
    NoExternalEntities,
    /// `DAV:preserved-live-properties` (RFC 4918 §16).
    PreservedLiveProperties,
    /// `DAV:propfind-finite-depth` (RFC 4918 §16).
    PropfindFiniteDepth,
    /// `DAV:cannot-modify-protected-property` (RFC 4918 §16).
    CannotModifyProtectedProperty,
    /// `DAV:resource-must-be-null` (RFC 5689 §3).
    ResourceMustBeNull,
    /// `DAV:valid-resourcetype` (RFC 5689 §3).
    ValidResourcetype,
    /// `DAV:supported-report` (RFC 3253 §3.6).
    SupportedReport,
    /// `DAV:number-of-matches-within-limits` (RFC 3253 §3.6, RFC 6578
    /// §3.6).
    NumberOfMatchesWithinLimits,
    /// `DAV:valid-sync-token` (RFC 6578 §3.2).
    ValidSyncToken,
    /// `C:supported-calendar-data` (RFC 4791 §5.3.2.1).
    CalendarSupportedData,
    /// `C:valid-calendar-data` (RFC 4791 §5.3.2.1).
    CalendarValidData,
    /// `C:valid-calendar-object-resource` (RFC 4791 §5.3.2.1).
    CalendarValidObjectResource,
    /// `C:supported-calendar-component` (RFC 4791 §5.3.2.1).
    CalendarSupportedComponent,
    /// `C:no-uid-conflict`, with the href of the resource holding the UID
    /// (RFC 4791 §5.3.2.1).
    CalendarNoUidConflict(String),
    /// `C:calendar-collection-location-ok` (RFC 4791 §5.3.2.1).
    CalendarCollectionLocationOk,
    /// `C:max-resource-size` (RFC 4791 §5.3.2.1).
    CalendarMaxResourceSize,
    /// `C:min-date-time` (RFC 4791 §5.3.2.1).
    CalendarMinDateTime,
    /// `C:max-date-time` (RFC 4791 §5.3.2.1).
    CalendarMaxDateTime,
    /// `C:max-instances` (RFC 4791 §5.3.2.1).
    CalendarMaxInstances,
    /// `C:max-attendees-per-instance` (RFC 4791 §5.3.2.1).
    CalendarMaxAttendeesPerInstance,
    /// `C:valid-filter` (RFC 4791 §7.8).
    CalendarValidFilter,
    /// `C:supported-filter` (RFC 4791 §7.8).
    CalendarSupportedFilter,
    /// `C:supported-collation` (RFC 4791 §7.5.1).
    CalendarSupportedCollation,
    /// `CR:supported-address-data` (RFC 6352 §6.3.2.1).
    AddressbookSupportedData,
    /// `CR:valid-address-data` (RFC 6352 §6.3.2.1).
    AddressbookValidData,
    /// `CR:no-uid-conflict`, with the href of the resource holding the UID
    /// (RFC 6352 §6.3.2.1).
    AddressbookNoUidConflict(String),
    /// `CR:addressbook-collection-location-ok` (RFC 6352 §6.3.2.1).
    AddressbookCollectionLocationOk,
    /// `CR:max-resource-size` (RFC 6352 §6.3.2.1).
    AddressbookMaxResourceSize,
    /// `CR:supported-address-data-conversion` (RFC 6352 §8.7).
    AddressbookSupportedDataConversion,
    /// `CR:supported-filter` (RFC 6352 §8.6).
    AddressbookSupportedFilter,
    /// `CR:supported-collation` (RFC 6352 §8.3).
    AddressbookSupportedCollation,
}

impl WebdavPrecondition {
    /// Writes the condition element, under the prefixes of
    /// [`NAMESPACE_DECLS`].
    pub fn to_xml(&self) -> String {
        let (name, hrefs): (&str, &[String]) = match self {
            Self::LockTokenMatchesRequestUri => ("D:lock-token-matches-request-uri", &[]),
            Self::LockTokenSubmitted(hrefs) => ("D:lock-token-submitted", hrefs),
            Self::NoConflictingLock(hrefs) => ("D:no-conflicting-lock", hrefs),
            Self::NoExternalEntities => ("D:no-external-entities", &[]),
            Self::PreservedLiveProperties => ("D:preserved-live-properties", &[]),
            Self::PropfindFiniteDepth => ("D:propfind-finite-depth", &[]),
            Self::CannotModifyProtectedProperty => ("D:cannot-modify-protected-property", &[]),
            Self::ResourceMustBeNull => ("D:resource-must-be-null", &[]),
            Self::ValidResourcetype => ("D:valid-resourcetype", &[]),
            Self::SupportedReport => ("D:supported-report", &[]),
            Self::NumberOfMatchesWithinLimits => ("D:number-of-matches-within-limits", &[]),
            Self::ValidSyncToken => ("D:valid-sync-token", &[]),
            Self::CalendarSupportedData => ("C:supported-calendar-data", &[]),
            Self::CalendarValidData => ("C:valid-calendar-data", &[]),
            Self::CalendarValidObjectResource => ("C:valid-calendar-object-resource", &[]),
            Self::CalendarSupportedComponent => ("C:supported-calendar-component", &[]),
            Self::CalendarNoUidConflict(href) => ("C:no-uid-conflict", slice::from_ref(href)),
            Self::CalendarCollectionLocationOk => ("C:calendar-collection-location-ok", &[]),
            Self::CalendarMaxResourceSize => ("C:max-resource-size", &[]),
            Self::CalendarMinDateTime => ("C:min-date-time", &[]),
            Self::CalendarMaxDateTime => ("C:max-date-time", &[]),
            Self::CalendarMaxInstances => ("C:max-instances", &[]),
            Self::CalendarMaxAttendeesPerInstance => ("C:max-attendees-per-instance", &[]),
            Self::CalendarValidFilter => ("C:valid-filter", &[]),
            Self::CalendarSupportedFilter => ("C:supported-filter", &[]),
            Self::CalendarSupportedCollation => ("C:supported-collation", &[]),
            Self::AddressbookSupportedData => ("CR:supported-address-data", &[]),
            Self::AddressbookValidData => ("CR:valid-address-data", &[]),
            Self::AddressbookNoUidConflict(href) => ("CR:no-uid-conflict", slice::from_ref(href)),
            Self::AddressbookCollectionLocationOk => ("CR:addressbook-collection-location-ok", &[]),
            Self::AddressbookMaxResourceSize => ("CR:max-resource-size", &[]),
            Self::AddressbookSupportedDataConversion => {
                ("CR:supported-address-data-conversion", &[])
            }
            Self::AddressbookSupportedFilter => ("CR:supported-filter", &[]),
            Self::AddressbookSupportedCollation => ("CR:supported-collation", &[]),
        };

        if hrefs.is_empty() {
            return format!("<{name}/>");
        }

        let mut xml = format!("<{name}>");
        for href in hrefs {
            xml.push_str(&format!("<D:href>{}</D:href>", escape_text(href)));
        }
        xml.push_str(&format!("</{name}>"));
        xml
    }

    /// Writes the whole `DAV:error` body a failed request answers with
    /// (RFC 4918 §8.7).
    pub fn error_body(&self) -> String {
        let condition = self.to_xml();
        format!("{XML_DECL}<D:error{NAMESPACE_DECLS}>{condition}</D:error>")
    }
}
