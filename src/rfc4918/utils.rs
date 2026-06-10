//! WebDAV shared helpers: the `Authorization` header emitter, request
//! path resolution and `ETag` extraction.

use alloc::{
    format,
    string::{String, ToString},
};

use base64::{Engine, prelude::BASE64_STANDARD};
use io_http::rfc9110::response::HttpResponse;
use secrecy::ExposeSecret;
use url::Url;

use crate::rfc4918::WebdavAuth;

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

/// Resolves `path` against `base_url`.
///
/// Empty paths return `base_url` unchanged. Absolute paths (starting
/// with `/`) replace the base path. Relative paths are appended to the
/// base path. Falls back to `base_url` when the join fails, preserving
/// the legacy push-path semantics.
pub fn resolve(base_url: &Url, path: &str) -> Url {
    if path.is_empty() {
        return base_url.clone();
    }

    if path.starts_with('/') {
        if let Ok(mut url) = Url::parse(base_url.as_str()) {
            url.set_path(path);
            return url;
        }
    }

    let mut base = base_url.clone();
    if !base.path().ends_with('/') {
        let mut new_path = base.path().to_string();
        new_path.push('/');
        base.set_path(&new_path);
    }

    base.join(path).unwrap_or_else(|_| base_url.clone())
}

/// Reads the `ETag` header (RFC 9110 §8.8.3) out of an HTTP response,
/// stripping the surrounding double quotes when present. Useful for
/// callers that want to thread the post-`PUT` ETag into their cache.
pub fn read_etag(response: &HttpResponse) -> Option<String> {
    response
        .header("etag")
        .map(|raw| raw.trim_matches('"').into())
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use secrecy::SecretString;

    use crate::rfc4918::{WebdavAuth, utils::*};

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
        // NOTE: base64("alice:secret") = "YWxpY2U6c2VjcmV0"
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
