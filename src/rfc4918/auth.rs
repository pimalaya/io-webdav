//! WebDAV authentication schemes and `Authorization` header emitter.
//!
//! Covers the three modes the CalDAV/CardDAV deployments handle in
//! practice: no auth, HTTP Basic (RFC 7617) and HTTP Bearer (RFC 6750).
//! Higher-level coroutines never observe the credential directly; they
//! only see the pre-formatted header value from [`emit_header`].

use alloc::{
    format,
    string::{String, ToString},
};

use base64::{Engine, prelude::BASE64_STANDARD};
use secrecy::{ExposeSecret, SecretString};

/// Authentication scheme used by the WebDAV client.
#[derive(Clone, Debug, Default)]
pub enum WebdavAuth {
    /// No authentication; no `Authorization` header is emitted.
    #[default]
    None,

    /// HTTP Basic authentication (RFC 7617).
    Basic {
        username: String,
        password: SecretString,
    },

    /// HTTP Bearer authentication (RFC 6750).
    Bearer { token: SecretString },
}

/// Returns the value of the HTTP `Authorization` header for the given
/// scheme, or [`None`] when no header should be emitted.
pub fn emit_header(auth: &WebdavAuth) -> Option<String> {
    match auth {
        WebdavAuth::None => None,
        WebdavAuth::Basic { username, password } => {
            let password = password.expose_secret();
            let digest = BASE64_STANDARD.encode(format!("{username}:{password}"));
            Some(format!("Basic {digest}"))
        }
        WebdavAuth::Bearer { token } => Some(format!("Bearer {}", token.expose_secret())),
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use crate::rfc4918::auth::*;

    #[test]
    fn none_emits_nothing() {
        assert!(emit_header(&WebdavAuth::None).is_none());
    }

    #[test]
    fn basic_encodes_credentials() {
        let auth = WebdavAuth::Basic {
            username: "alice".into(),
            password: SecretString::from("secret".to_string()),
        };
        let header = emit_header(&auth).unwrap();
        // base64("alice:secret") = "YWxpY2U6c2VjcmV0"
        assert_eq!(header, "Basic YWxpY2U6c2VjcmV0");
    }

    #[test]
    fn bearer_prepends_scheme() {
        let auth = WebdavAuth::Bearer {
            token: SecretString::from("xyz".to_string()),
        };
        assert_eq!(emit_header(&auth).unwrap(), "Bearer xyz");
    }
}
