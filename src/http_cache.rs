use log::debug;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(14695981039346656037u64, |h, b| {
        (h ^ b as u64).wrapping_mul(1099511628211)
    })
}

fn resolve_cache_dir() -> Option<PathBuf> {
    std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|xdg| Path::new(xdg).is_absolute())
        .map(|xdg| PathBuf::from(xdg).join("gitignore-in"))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache").join("gitignore-in"))
        })
}

fn body_path(dir: &Path, hash: u64) -> PathBuf {
    dir.join(format!("{hash:016x}.body"))
}

fn meta_path(dir: &Path, hash: u64) -> PathBuf {
    dir.join(format!("{hash:016x}.meta"))
}

pub struct CacheEntry {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body: String,
}

fn get_from_dir(dir: &Path, url: &str) -> Option<CacheEntry> {
    let hash = fnv1a(url);
    let body = std::fs::read_to_string(body_path(dir, hash)).ok()?;
    let meta = std::fs::read_to_string(meta_path(dir, hash))
        .ok()
        .unwrap_or_default();
    let mut etag = None;
    let mut last_modified = None;
    for line in meta.lines() {
        if let Some(v) = line.strip_prefix("etag: ") {
            etag = Some(v.to_owned());
        } else if let Some(v) = line.strip_prefix("last-modified: ") {
            last_modified = Some(v.to_owned());
        }
    }
    Some(CacheEntry {
        etag,
        last_modified,
        body,
    })
}

fn put_to_dir(dir: &Path, url: &str, entry: &CacheEntry) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        debug!("http_cache: cannot create {}: {e}", dir.display());
        return;
    }
    let hash = fnv1a(url);
    if let Err(e) = atomic_write(dir, &body_path(dir, hash), entry.body.as_bytes()) {
        debug!("http_cache: cannot write body for {url}: {e}");
        return;
    }
    let mut meta = String::new();
    if let Some(etag) = &entry.etag {
        meta.push_str("etag: ");
        meta.push_str(etag);
        meta.push('\n');
    }
    if let Some(lm) = &entry.last_modified {
        meta.push_str("last-modified: ");
        meta.push_str(lm);
        meta.push('\n');
    }
    if let Err(e) = atomic_write(dir, &meta_path(dir, hash), meta.as_bytes()) {
        debug!("http_cache: cannot write meta for {url}: {e}");
    }
}

pub fn get(url: &str) -> Option<CacheEntry> {
    get_from_dir(&resolve_cache_dir()?, url)
}

pub fn put(url: &str, entry: &CacheEntry) {
    let Some(dir) = resolve_cache_dir() else {
        return;
    };
    put_to_dir(&dir, url, entry);
}

fn atomic_write(dir: &Path, dest: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.persist(dest).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsString,
        sync::{Mutex, OnceLock},
    };

    fn environment_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvironmentGuard {
        xdg_cache_home: Option<OsString>,
        home: Option<OsString>,
    }

    impl EnvironmentGuard {
        fn capture() -> Self {
            Self {
                xdg_cache_home: std::env::var_os("XDG_CACHE_HOME"),
                home: std::env::var_os("HOME"),
            }
        }
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            restore_environment_variable("XDG_CACHE_HOME", self.xdg_cache_home.take());
            restore_environment_variable("HOME", self.home.take());
        }
    }

    fn restore_environment_variable(name: &str, value: Option<OsString>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn test_fnv1a_differs_by_url() {
        assert_ne!(
            fnv1a("https://example.test/api/Rust"),
            fnv1a("https://example.test/api/Go")
        );
    }

    #[test]
    fn test_fnv1a_empty_string() {
        // FNV-1a of empty string is the offset basis itself
        assert_eq!(fnv1a(""), 14695981039346656037u64);
    }

    #[test]
    fn test_cache_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let url = "https://example.test/api/Rust";
        let entry = CacheEntry {
            etag: Some(r#""abc123""#.to_owned()),
            last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".to_owned()),
            body: "### Rust ###\nfoo\n".to_owned(),
        };
        put_to_dir(tmp.path(), url, &entry);
        let got = get_from_dir(tmp.path(), url).expect("cache should have an entry after put");
        assert_eq!(got.body, entry.body);
        assert_eq!(got.etag, entry.etag);
        assert_eq!(got.last_modified, entry.last_modified);
    }

    #[test]
    fn test_cache_miss_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let result = get_from_dir(tmp.path(), "https://example.test/api/nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_entry_without_etag() {
        let tmp = tempfile::tempdir().unwrap();
        let url = "https://example.test/api/Python";
        let entry = CacheEntry {
            etag: None,
            last_modified: None,
            body: "### Python ###\nfoo\n".to_owned(),
        };
        put_to_dir(tmp.path(), url, &entry);
        let got = get_from_dir(tmp.path(), url).expect("cache should have an entry after put");
        assert_eq!(got.body, entry.body);
        assert!(got.etag.is_none());
        assert!(got.last_modified.is_none());
    }

    #[test]
    fn test_cache_isolates_different_urls() {
        let tmp = tempfile::tempdir().unwrap();
        let url_a = "https://example.test/api/Rust";
        let url_b = "https://example.test/api/Go";
        put_to_dir(
            tmp.path(),
            url_a,
            &CacheEntry {
                etag: None,
                last_modified: None,
                body: "### Rust ###".to_owned(),
            },
        );
        assert!(get_from_dir(tmp.path(), url_b).is_none());
        assert!(get_from_dir(tmp.path(), url_a).is_some());
    }

    #[test]
    fn test_resolve_cache_dir_uses_absolute_xdg_cache_home() {
        let _lock = environment_lock().lock().unwrap();
        let _environment = EnvironmentGuard::capture();
        let xdg = tempfile::tempdir().unwrap();

        std::env::set_var("XDG_CACHE_HOME", xdg.path());
        std::env::set_var("HOME", "/unused-home");

        assert_eq!(resolve_cache_dir(), Some(xdg.path().join("gitignore-in")));
    }

    #[test]
    fn test_resolve_cache_dir_ignores_empty_and_relative_xdg_cache_home() {
        let _lock = environment_lock().lock().unwrap();
        let _environment = EnvironmentGuard::capture();
        let home = tempfile::tempdir().unwrap();
        let expected = home.path().join(".cache").join("gitignore-in");

        std::env::set_var("HOME", home.path());
        for xdg in ["", "relative-cache"] {
            std::env::set_var("XDG_CACHE_HOME", xdg);
            assert_eq!(resolve_cache_dir(), Some(expected.clone()));
        }
    }
}
