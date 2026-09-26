//! # Query filters
//!
//! The `C:filter` element of an `addressbook-query` REPORT (RFC 6352 §10.5),
//! modelled whole and serialised by [`CarddavFilter::to_xml`].
//!
//! The server evaluates it. The crate only says what to match, and every test
//! is sent explicitly so the schema default never applies.

use alloc::{format, string::String, vec::Vec};

use crate::rfc4918::{escape_attr, escape_text};

/// The `C:filter` element (RFC 6352 §10.5).
///
/// The default is the empty `allof`, which matches every card: RFC 6352 §8.6
/// requires a filter, and strict servers (Google) read an empty `anyof`, the
/// schema default, as matching nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarddavFilter {
    /// How the prop-filters combine.
    pub test: CarddavFilterTest,
    /// The prop-filters, each testing one vCard property.
    pub props: Vec<CarddavPropFilter>,
}

impl Default for CarddavFilter {
    fn default() -> Self {
        Self {
            test: CarddavFilterTest::AllOf,
            props: Vec::new(),
        }
    }
}

impl CarddavFilter {
    /// Serialises the filter, CardDAV elements under the `C` prefix.
    pub fn to_xml(&self) -> String {
        let mut xml = format!("<C:filter test=\"{}\">", self.test.as_str());
        for prop in &self.props {
            xml.push_str(&prop.to_xml());
        }
        xml.push_str("</C:filter>");
        xml
    }
}

/// The `test` attribute of a filter or prop-filter (RFC 6352 §10.5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarddavFilterTest {
    /// Matches when any child matches.
    AnyOf,
    /// Matches when every child matches.
    AllOf,
}

impl CarddavFilterTest {
    /// The attribute value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AnyOf => "anyof",
            Self::AllOf => "allof",
        }
    }
}

/// The `C:prop-filter` element, testing one vCard property (RFC 6352
/// §10.5.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarddavPropFilter {
    /// The property name, such as `EMAIL` or `FN`.
    pub name: String,
    /// How the text-matches and param-filters combine.
    pub test: CarddavFilterTest,
    /// What the property must satisfy.
    pub cond: CarddavPropCond,
}

impl CarddavPropFilter {
    fn to_xml(&self) -> String {
        let name = escape_attr(&self.name);
        let test = self.test.as_str();
        let mut xml = format!("<C:prop-filter name=\"{name}\" test=\"{test}\">");

        match &self.cond {
            CarddavPropCond::IsNotDefined => xml.push_str("<C:is-not-defined/>"),
            CarddavPropCond::Match { texts, params } => {
                for text in texts {
                    xml.push_str(&text.to_xml());
                }
                for param in params {
                    xml.push_str(&param.to_xml());
                }
            }
        }

        xml.push_str("</C:prop-filter>");
        xml
    }
}

/// What a prop-filter tests (RFC 6352 §10.5.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CarddavPropCond {
    /// The property is absent.
    IsNotDefined,
    /// The property satisfies the text-matches and param-filters. Both empty,
    /// it only has to be present.
    Match {
        /// Matches against the property value.
        texts: Vec<CarddavTextMatch>,
        /// Tests on the property parameters.
        params: Vec<CarddavParamFilter>,
    },
}

/// The `C:param-filter` element, testing one property parameter (RFC 6352
/// §10.5.2).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarddavParamFilter {
    /// The parameter name, such as `TYPE`.
    pub name: String,
    /// What the parameter must satisfy, [`None`] when it only has to be
    /// present.
    pub cond: Option<CarddavParamCond>,
}

impl CarddavParamFilter {
    fn to_xml(&self) -> String {
        let name = escape_attr(&self.name);

        match &self.cond {
            None => format!("<C:param-filter name=\"{name}\"/>"),
            Some(CarddavParamCond::IsNotDefined) => {
                format!("<C:param-filter name=\"{name}\"><C:is-not-defined/></C:param-filter>")
            }
            Some(CarddavParamCond::TextMatch(text)) => {
                let text = text.to_xml();
                format!("<C:param-filter name=\"{name}\">{text}</C:param-filter>")
            }
        }
    }
}

/// What a param-filter tests (RFC 6352 §10.5.2).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CarddavParamCond {
    /// The parameter is absent.
    IsNotDefined,
    /// The parameter value matches.
    TextMatch(CarddavTextMatch),
}

/// The `C:text-match` element (RFC 6352 §10.5.4).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CarddavTextMatch {
    /// The text to match.
    pub value: String,
    /// How the value is compared.
    pub match_type: CarddavMatchType,
    /// Whether the match is inverted (`negate-condition="yes"`).
    pub negate: bool,
    /// The collation (RFC 4790), [`None`] leaving the server's default,
    /// `i;unicode-casemap` (RFC 6352 §8.3).
    pub collation: Option<String>,
}

impl CarddavTextMatch {
    fn to_xml(&self) -> String {
        let mut xml = String::from("<C:text-match");

        if let Some(collation) = &self.collation {
            xml.push_str(&format!(" collation=\"{}\"", escape_attr(collation)));
        }

        if self.negate {
            xml.push_str(" negate-condition=\"yes\"");
        }

        let match_type = self.match_type.as_str();
        let value = escape_text(&self.value);
        xml.push_str(&format!(
            " match-type=\"{match_type}\">{value}</C:text-match>"
        ));
        xml
    }
}

/// The `match-type` attribute of a text-match (RFC 6352 §10.5.4).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CarddavMatchType {
    /// The whole value equals the text.
    Equals,
    /// The value contains the text, the RFC default.
    #[default]
    Contains,
    /// The value starts with the text.
    StartsWith,
    /// The value ends with the text.
    EndsWith,
}

impl CarddavMatchType {
    /// The attribute value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::Contains => "contains",
            Self::StartsWith => "starts-with",
            Self::EndsWith => "ends-with",
        }
    }
}
