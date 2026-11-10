//! Loads keymap-drawer SVGs, the cheat sheet of each keyboard.

use crate::config::KeyboardConfig;
use std::path::{Path, PathBuf};
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Keymap {
    /// The URL or path this was loaded from.
    pub source: String,
    pub svg: Option<String>,
    /// Layer names in keymap order, so a layer index from the firmware maps to a name.
    pub layers: Vec<String>,
    pub error: Option<String>,
    /// GitHub's ETag of `svg`; sent back with If-None-Match to check for a newer file.
    pub etag: Option<String>,
    /// Goes up by one whenever `svg` changes, so the popover knows to redraw.
    pub revision: u32,
    /// When the source was last read or checked successfully (ms since the epoch)
    pub checked_at: Option<u64>,
}

/// Brings a keyboard's keymap up to date with its source.
///
/// URLs are checked with If-None-Match: an unchanged file costs a 304 with no body. The SVG
/// and its ETag are cached on disk, so the app starts with the last copy and works offline.
/// Returns the keymap and whether anything the app shows changed.
pub fn refresh(keyboard: &KeyboardConfig, previous: Option<&Keymap>, cache_dir: &Path) -> (Keymap, bool) {
    let source = keyboard.keymap.trim().to_string();
    let mut keymap = match previous {
        Some(p) if p.source == source => p.clone(),
        // New keyboard or new source: start over, keeping the revision counting up
        _ => Keymap { source: source.clone(), revision: previous.map_or(0, |p| p.revision + 1), ..Default::default() },
    };

    if source.is_empty() {
        keymap.svg = None;
        keymap.error = None;
    } else if source.starts_with("http://") || source.starts_with("https://") {
        let cache = Cache::new(cache_dir, &keyboard.name);
        if keymap.svg.is_none() {
            if let Some((svg, etag)) = cache.read(&source) {
                keymap.svg = Some(svg);
                keymap.etag = etag;
                keymap.revision += 1;
            }
        }
        let etag = keymap.svg.as_ref().and(keymap.etag.as_deref());
        match fetch(&source, etag) {
            Ok(Fetched::NotModified) => {
                keymap.error = None;
                keymap.checked_at = Some(crate::hub::now_ms());
            }
            Ok(Fetched::Body { svg, etag }) => {
                cache.write(&source, &svg, etag.as_deref());
                if keymap.svg.as_ref() != Some(&svg) {
                    keymap.svg = Some(svg);
                    keymap.revision += 1;
                }
                keymap.etag = etag;
                keymap.error = None;
                keymap.checked_at = Some(crate::hub::now_ms());
            }
            Err(e) if keymap.svg.is_some() => keymap.error = Some(format!("Offline, showing the saved copy: {e}")),
            Err(e) => keymap.error = Some(e),
        }
    } else {
        let path = expand_home(&source);
        match std::fs::read_to_string(&path) {
            Ok(svg) => {
                if keymap.svg.as_ref() != Some(&svg) {
                    keymap.svg = Some(svg);
                    keymap.revision += 1;
                }
                keymap.error = None;
                keymap.checked_at = Some(crate::hub::now_ms());
            }
            Err(e) => keymap.error = Some(format!("{}: {e}", path.display())),
        }
    }

    keymap.layers = if !keyboard.layer_names.is_empty() {
        keyboard.layer_names.clone()
    } else {
        keymap.svg.as_deref().map(layer_names).unwrap_or_default()
    };
    let changed = previous.is_none_or(|p| {
        p.revision != keymap.revision || p.layers != keymap.layers || p.error != keymap.error
    });
    (keymap, changed)
}

/// The last downloaded SVG of a keyboard and its ETag, as `<name>.svg` and `<name>.etag`.
/// The .etag file holds the source URL on its first line, so a changed URL is not mixed up
/// with the old file.
struct Cache {
    svg: PathBuf,
    meta: PathBuf,
}

impl Cache {
    fn new(dir: &Path, keyboard: &str) -> Cache {
        let stem = file_stem(keyboard);
        Cache { svg: dir.join(format!("{stem}.svg")), meta: dir.join(format!("{stem}.etag")) }
    }

    fn read(&self, source: &str) -> Option<(String, Option<String>)> {
        let svg = std::fs::read_to_string(&self.svg).ok()?;
        let meta = std::fs::read_to_string(&self.meta).unwrap_or_default();
        let mut lines = meta.lines();
        match lines.next() {
            Some(url) if url == source => Some((svg, lines.next().filter(|e| !e.is_empty()).map(String::from))),
            // A copy from before ETags were saved: show it, but download the file again
            None => Some((svg, None)),
            Some(_) => None,
        }
    }

    fn write(&self, source: &str, svg: &str, etag: Option<&str>) {
        if let Some(dir) = self.svg.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.svg, svg);
        let _ = std::fs::write(&self.meta, format!("{source}\n{}\n", etag.unwrap_or("")));
    }
}

enum Fetched {
    NotModified,
    Body { svg: String, etag: Option<String> },
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

/// keymap-drawer puts each layer in `<g ... class="layer-NAME">`, in keymap order.
pub fn layer_names(svg: &str) -> Vec<String> {
    const MARKER: &str = "class=\"layer-";
    let mut names: Vec<String> = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find(MARKER) {
        rest = &rest[start + MARKER.len()..];
        let Some(end) = rest.find('"') else { break };
        let name = unescape(&rest[..end]);
        if !names.contains(&name) {
            names.push(name);
        }
        rest = &rest[end..];
    }
    names
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

fn file_stem(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard(source: &str) -> KeyboardConfig {
        KeyboardConfig { name: "Test".into(), keymap: source.into(), ..Default::default() }
    }

    #[test]
    fn a_file_keymap_gets_a_new_revision_only_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("keylay-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("k.svg");
        let kb = keyboard(file.to_str().unwrap());

        std::fs::write(&file, r#"<g class="layer-Base"></g>"#).unwrap();
        let (first, changed) = refresh(&kb, None, &dir);
        assert!(changed);
        assert_eq!(first.layers, vec!["Base"]);

        let (same, changed) = refresh(&kb, Some(&first), &dir);
        assert!(!changed);
        assert_eq!(same.revision, first.revision);

        std::fs::write(&file, r#"<g class="layer-Base"></g><g class="layer-Lower"></g>"#).unwrap();
        let (second, changed) = refresh(&kb, Some(&same), &dir);
        assert!(changed);
        assert_eq!(second.revision, first.revision + 1);
        assert_eq!(second.layers, vec!["Base", "Lower"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Talks to GitHub: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn github_answers_304_for_an_unchanged_file() {
        let dir = std::env::temp_dir().join(format!("keylay-net-{}", std::process::id()));
        let kb = keyboard("https://raw.githubusercontent.com/codeyoma/keyboard-zmk-sweep/main/sweep_keymap.svg");
        let (first, _) = refresh(&kb, None, &dir);
        assert!(first.svg.is_some() && first.etag.is_some(), "{:?}", first.error);
        assert!(matches!(fetch(&kb.keymap, first.etag.as_deref()), Ok(Fetched::NotModified)));
        let (second, changed) = refresh(&kb, Some(&first), &dir);
        assert!(!changed && second.error.is_none());

        // A fresh start reads the cache with its ETag, and the check is again a 304
        let (cached, _) = refresh(&kb, None, &dir);
        assert_eq!(cached.etag, first.etag);
        assert!(cached.error.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_layer_names_in_order() {
        let svg = r#"<svg><style>text.layer-activator { }</style>
            <g transform="translate(30, 0)" class="layer-yoma"><text class="key tap layer-activator">x</text></g>
            <g transform="translate(30, 280)" class="layer-ARST"></g>
            <g transform="translate(30, 560)" class="layer-Lower"></g></svg>"#;
        assert_eq!(layer_names(svg), vec!["yoma", "ARST", "Lower"]);
    }
}
