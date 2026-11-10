//! Where keymap SVGs are read from. [`Sources`] reads web addresses (with an ETag check) and
//! files; tests put a fake behind the same [`Reader`] interface.

use std::path::PathBuf;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    /// The ETag still matches (HTTP 304)
    NotModified,
    Body { svg: String, etag: Option<String> },
}

pub trait Reader: Send + Sync {
    /// Reads `source`, a web address or a file path. `etag` is the ETag of the copy the caller
    /// has; a web source answers `NotModified` when it still matches.
    fn read(&self, source: &str, etag: Option<&str>) -> Result<Fetched, String>;
}

pub fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

/// The real sources: HTTP(S) and files (`~` allowed).
pub struct Sources;

impl Reader for Sources {
    fn read(&self, source: &str, etag: Option<&str>) -> Result<Fetched, String> {
        if is_url(source) {
            fetch(source, etag)
        } else {
            let path = expand_home(source);
            let svg = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(Fetched::Body { svg, etag: None })
        }
    }
}

fn fetch(url: &str, etag: Option<&str>) -> Result<Fetched, String> {
    let tls = TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(10)))
        // ureq treats every 3xx as a redirect to follow, 304 included; get it back as is
        .max_redirects(0)
        .build()
        .into();
    let mut request = agent.get(url);
    if let Some(etag) = etag {
        request = request.header("If-None-Match", etag);
    }
    let mut response = request.call().map_err(|e| format!("{url}: {e}"))?;
    match response.status().as_u16() {
        304 => return Ok(Fetched::NotModified),
        300..=399 => return Err(format!("{url}: moved ({}), update the URL in the config", response.status())),
        _ => {}
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let svg = response.body_mut().read_to_string().map_err(|e| format!("{url}: {e}"))?;
    Ok(Fetched::Body { svg, etag })
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

