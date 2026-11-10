//! Keymap-drawer SVGs, the cheat sheet of each keyboard: the [`Store`] keeps one per keyboard,
//! checks them against their source (an unchanged file on GitHub costs a 304), caches web
//! copies on disk, and counts a revision up whenever an SVG changes.

mod source;

use crate::config::KeyboardConfig;
use source::{is_url, Fetched, Reader, Sources};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

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
fn update(
    keyboard: &KeyboardConfig,
    previous: Option<&Keymap>,
    cache_dir: &Path,
    reader: &dyn Reader,
) -> (Keymap, bool) {
    let source = keyboard.keymap.trim().to_string();
    let mut keymap = match previous {
        Some(p) if p.source == source => p.clone(),
        // New keyboard or new source: start over, keeping the revision counting up
        _ => Keymap { source: source.clone(), revision: previous.map_or(0, |p| p.revision + 1), ..Default::default() },
    };

    if source.is_empty() {
        keymap.svg = None;
        keymap.error = None;
    } else if is_url(&source) {
        let cache = Cache::new(cache_dir, &keyboard.name);
        if keymap.svg.is_none() {
            if let Some((svg, etag)) = cache.read(&source) {
                keymap.svg = Some(svg);
                keymap.etag = etag;
                keymap.revision += 1;
            }
        }
        let etag = keymap.svg.as_ref().and(keymap.etag.as_deref());
        match reader.read(&source, etag) {
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
        match reader.read(&source, None) {
            Ok(Fetched::Body { svg, .. }) => {
                if keymap.svg.as_ref() != Some(&svg) {
                    keymap.svg = Some(svg);
                    keymap.revision += 1;
                }
                keymap.error = None;
                keymap.checked_at = Some(crate::hub::now_ms());
            }
            Ok(Fetched::NotModified) => {}
            Err(e) => keymap.error = Some(e),
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

/// The keymaps of every keyboard in the config, and when they were last checked.
pub struct Store {
    cache_dir: PathBuf,
    reader: Box<dyn Reader>,
    keymaps: RwLock<HashMap<String, Keymap>>,
    /// Held for the whole check, so two checks never run at once
    last_check: Mutex<Option<Instant>>,
}

impl Store {
    pub fn new(cache_dir: PathBuf) -> Store {
        Store::with_reader(cache_dir, Box::new(Sources))
    }

    fn with_reader(cache_dir: PathBuf, reader: Box<dyn Reader>) -> Store {
        Store { cache_dir, reader, keymaps: RwLock::new(HashMap::new()), last_check: Mutex::new(None) }
    }

    /// Checks every keyboard's keymap against its source; keyboards no longer in the config
    /// are dropped. Blocks on the network. Returns whether anything the app shows changed.
    pub fn refresh(&self, keyboards: &[KeyboardConfig]) -> bool {
        let mut last_check = self.last_check.lock().unwrap();
        let previous = self.keymaps.read().unwrap().clone();
        let mut changed = previous.len() != keyboards.len();
        let keymaps: HashMap<String, Keymap> = keyboards
            .iter()
            .map(|k| {
                let (keymap, c) = update(k, previous.get(&k.name), &self.cache_dir, self.reader.as_ref());
                changed |= c;
                (k.name.clone(), keymap)
            })
            .collect();
        *self.keymaps.write().unwrap() = keymaps;
        *last_check = Some(Instant::now());
        changed
    }

    /// Whether a check ran in the last `age`, or is running now.
    pub fn checked_within(&self, age: Duration) -> bool {
        match self.last_check.try_lock() {
            Ok(last) => last.is_some_and(|t| t.elapsed() < age),
            Err(_) => true,
        }
    }

    pub fn all(&self) -> RwLockReadGuard<'_, HashMap<String, Keymap>> {
        self.keymaps.read().unwrap()
    }

    pub fn svg(&self, keyboard: &str) -> Option<String> {
        self.all().get(keyboard).and_then(|k| k.svg.clone())
    }
}

/// Reads a keymap source for the settings window, without caching it: the layer names, or
/// what went wrong.
pub fn probe(source: &str) -> Result<Vec<String>, String> {
    let svg = match Sources.read(source.trim(), None)? {
        Fetched::Body { svg, .. } => svg,
        Fetched::NotModified => return Err("unexpected 304".into()),
    };
    let layers = layer_names(&svg);
    if layers.is_empty() {
        return Err("No keymap-drawer layers in this file".into());
    }
    Ok(layers)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn keyboard(source: &str) -> KeyboardConfig {
        KeyboardConfig { name: "Test".into(), keymap: source.into(), ..Default::default() }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keylay-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A web source whose answers the test sets, and that remembers the ETags it was sent.
    #[derive(Default)]
    struct Fake {
        answer: Mutex<Option<Result<Fetched, String>>>,
        etags: Mutex<Vec<Option<String>>>,
    }

    impl Fake {
        fn answer(&self, answer: Result<Fetched, String>) {
            *self.answer.lock().unwrap() = Some(answer);
        }
    }

    impl Reader for Arc<Fake> {
        fn read(&self, _source: &str, etag: Option<&str>) -> Result<Fetched, String> {
            self.etags.lock().unwrap().push(etag.map(String::from));
            self.answer.lock().unwrap().clone().expect("no answer set")
        }
    }

    fn body(svg: &str, etag: &str) -> Result<Fetched, String> {
        Ok(Fetched::Body { svg: svg.into(), etag: Some(etag.into()) })
    }

    const URL: &str = "https://example.com/k.svg";

    #[test]
    fn a_web_keymap_is_checked_with_its_etag() {
        let dir = temp_dir("web");
        let fake = Arc::new(Fake::default());
        let store = Store::with_reader(dir.clone(), Box::new(fake.clone()));
        let kbs = [keyboard(URL)];

        fake.answer(body(r#"<g class="layer-Base"></g>"#, "\"a\""));
        assert!(store.refresh(&kbs));
        let first = store.all()["Test"].clone();
        assert_eq!(first.layers, vec!["Base"]);

        // Unchanged: a 304, same revision, nothing to redraw
        fake.answer(Ok(Fetched::NotModified));
        assert!(!store.refresh(&kbs));
        assert_eq!(store.all()["Test"].revision, first.revision);
        assert_eq!(fake.etags.lock().unwrap().last().unwrap().as_deref(), Some("\"a\""));

        // A new file: the next revision
        fake.answer(body(r#"<g class="layer-Base"></g><g class="layer-Num"></g>"#, "\"b\""));
        assert!(store.refresh(&kbs));
        assert_eq!(store.all()["Test"].revision, first.revision + 1);
        assert_eq!(store.all()["Test"].layers, vec!["Base", "Num"]);

        // Offline: the copy stays, with a note
        fake.answer(Err("no network".into()));
        assert!(store.refresh(&kbs));
        let offline = store.all()["Test"].clone();
        assert!(offline.svg.is_some());
        assert!(offline.error.unwrap().starts_with("Offline"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_start_uses_the_cached_copy_and_its_etag() {
        let dir = temp_dir("cache");
        let fake = Arc::new(Fake::default());
        fake.answer(body(r#"<g class="layer-Base"></g>"#, "\"a\""));
        Store::with_reader(dir.clone(), Box::new(fake.clone())).refresh(&[keyboard(URL)]);

        fake.answer(Err("no network".into()));
        let store = Store::with_reader(dir.clone(), Box::new(fake.clone()));
        store.refresh(&[keyboard(URL)]);
        assert_eq!(store.all()["Test"].layers, vec!["Base"]);
        assert_eq!(fake.etags.lock().unwrap().last().unwrap().as_deref(), Some("\"a\""));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_keymap_gets_a_new_revision_only_when_it_changes() {
        let dir = temp_dir("file");
        let file = dir.join("k.svg");
        let store = Store::new(dir.clone());
        let kbs = [keyboard(file.to_str().unwrap())];

        std::fs::write(&file, r#"<g class="layer-Base"></g>"#).unwrap();
        assert!(store.refresh(&kbs));
        let first = store.all()["Test"].revision;
        assert!(!store.refresh(&kbs));

        std::fs::write(&file, r#"<g class="layer-Base"></g><g class="layer-Lower"></g>"#).unwrap();
        assert!(store.refresh(&kbs));
        assert_eq!(store.all()["Test"].revision, first + 1);
        assert_eq!(store.all()["Test"].layers, vec!["Base", "Lower"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_removed_keyboard_is_dropped() {
        let dir = temp_dir("removed");
        let store = Store::new(dir.clone());
        store.refresh(&[keyboard("")]);
        assert!(store.refresh(&[]));
        assert!(store.all().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn checked_within_tracks_the_last_check() {
        let store = Store::new(temp_dir("age"));
        assert!(!store.checked_within(Duration::from_secs(60)));
        store.refresh(&[]);
        assert!(store.checked_within(Duration::from_secs(60)));
    }

    /// Talks to GitHub: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn github_answers_304_for_an_unchanged_file() {
        let url = "https://raw.githubusercontent.com/codeyoma/keyboard-zmk-sweep/main/sweep_keymap.svg";
        let store = Store::new(temp_dir("net"));
        store.refresh(&[keyboard(url)]);
        let first = store.all()["Test"].clone();
        assert!(first.svg.is_some() && first.etag.is_some(), "{:?}", first.error);
        assert_eq!(Sources.read(url, first.etag.as_deref()), Ok(Fetched::NotModified));
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
