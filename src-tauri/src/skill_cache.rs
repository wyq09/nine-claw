//! Lazily-refreshed per-root scan cache with a filesystem watcher.
//!
//! research-dsh report 03 step ①: every agent turn used to re-scan every
//! skill root directory with `fs::read_dir`. Now each root's scan result is
//! cached; filesystem watchers mark a root dirty on relevant changes and the
//! next read re-scans exactly that root on demand.
//!
//! Design invariants:
//! - invalidation only flips a flag — no scanning happens in the watcher
//!   thread, so event bursts are harmless and no debounce is needed;
//! - a broken/absent watcher can never hide skills: reads always fall back
//!   to the caller's scan closure when a root is dirty (which includes
//!   "never scanned");
//! - missing roots are watched from their nearest existing ancestor, and
//!   notify's recursive watches pick up segments created later.

use notify::Watcher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Process-lifetime handle that keeps the watcher alive; dropping it stops
/// watching.
pub struct SkillWatcherHandle {
    _watcher: Option<notify::RecommendedWatcher>,
}

pub struct SkillCache<T> {
    entries: RwLock<HashMap<PathBuf, Vec<T>>>,
    dirty: RwLock<HashSet<PathBuf>>,
}

impl<T: Clone> SkillCache<T> {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            dirty: RwLock::new(HashSet::new()),
        }
    }

    /// Return the cached entries for `root`, invoking `scan` exactly when the
    /// root is dirty (never scanned, invalidated, or re-created). `scan` must
    /// return the complete entry list for that root.
    pub fn get_or_scan<F>(&self, root: &Path, scan: F) -> Vec<T>
    where
        F: FnOnce() -> Vec<T>,
    {
        if !self.is_dirty(root) {
            if let Ok(entries) = self.entries.read() {
                if let Some(cached) = entries.get(root) {
                    return cached.clone();
                }
            }
        }

        let fresh = scan();
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(root.to_path_buf(), fresh.clone());
        }
        if let Ok(mut dirty) = self.dirty.write() {
            dirty.remove(root);
        }
        fresh
    }

    /// Mark `root` dirty; the next `get_or_scan` re-scans it.
    pub fn invalidate(&self, root: &Path) {
        if let Ok(mut dirty) = self.dirty.write() {
            dirty.insert(root.to_path_buf());
        }
    }

    /// Mark every known root dirty.
    #[allow(dead_code)] // 缓存 API 完整性：目前只有单 root 失效调用方
    pub fn invalidate_all(&self) {
        let known: Vec<PathBuf> = self
            .entries
            .read()
            .map(|entries| entries.keys().cloned().collect())
            .unwrap_or_default();
        for root in known {
            self.invalidate(&root);
        }
    }

    /// Observability helper (tests and diagnostics): true when the root
    /// would be re-scanned on the next read (dirty or never scanned).
    pub fn is_dirty(&self, root: &Path) -> bool {
        if self
            .dirty
            .read()
            .map(|dirty| dirty.contains(root))
            .unwrap_or(true)
        {
            return true;
        }
        self.entries
            .read()
            .map(|entries| !entries.contains_key(root))
            .unwrap_or(true)
    }
}

/// Watch the given skill root directories recursively. Any filesystem event
/// whose path lives under a root and whose file name is `SKILL.md` or ends
/// with `.md` marks that root dirty and emits the Tauri event
/// `"skills/change"` (unit payload) via `crate::emit_safe::emit_safe`.
///
/// Roots are canonicalized for matching but invalidated under their original
/// key, so callers can key the cache with the same paths they scanned.
///
/// The returned handle keeps the watcher alive; dropping it stops watching.
pub fn start_skill_watchers<R, T>(
    roots: Vec<PathBuf>,
    cache: Arc<SkillCache<T>>,
    app: &tauri::AppHandle<R>,
) -> SkillWatcherHandle
where
    R: tauri::Runtime,
    T: Clone + Send + Sync + 'static,
{
    let app = app.clone();
    // (original cache key, canonicalized watch path)
    let watch_roots: Vec<(PathBuf, PathBuf)> = roots
        .iter()
        .map(|root| {
            let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            (root.clone(), canonical)
        })
        .collect();
    let registration_roots = watch_roots.clone();

    let mut watcher = match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        match result {
            Ok(event) => handle_skill_event(&event, &watch_roots, &cache, &app),
            Err(error) => log::warn!("skill watcher 收到错误: {error}"),
        }
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            log::warn!("启动 skill watcher 失败: {error}");
            return SkillWatcherHandle { _watcher: None };
        }
    };

    for (_, watch_root) in &registration_roots {
        let target = nearest_existing_ancestor(watch_root);
        if let Err(error) = watcher.watch(&target, notify::RecursiveMode::Recursive) {
            log::warn!("监听技能根目录失败 {}: {error}", target.display());
        }
    }

    SkillWatcherHandle {
        _watcher: Some(watcher),
    }
}

fn handle_skill_event<R: tauri::Runtime, T: Clone + Send + Sync>(
    event: &notify::Event,
    watch_roots: &[(PathBuf, PathBuf)],
    cache: &SkillCache<T>,
    app: &tauri::AppHandle<R>,
) {
    // Reads/open/close churn is not a change signal.
    if matches!(event.kind, notify::EventKind::Access(_)) {
        return;
    }

    let mut changed = false;
    for path in &event.paths {
        let Some((original_root, _)) = watch_roots
            .iter()
            .find(|(_, watch_root)| path.starts_with(watch_root))
        else {
            continue;
        };
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name == "SKILL.md" || name.ends_with(".md") {
            cache.invalidate(original_root);
            changed = true;
        }
    }

    if changed {
        crate::emit_safe::emit_safe(app, "skills/change", ());
    }
}

fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return current;
        }
        let Some(parent) = current.parent() else {
            return path.to_path_buf();
        };
        if parent == current {
            return path.to_path_buf();
        }
        current = parent.to_path_buf();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-skill-cache-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn get_or_scan_scans_once_then_caches() {
        let cache = SkillCache::<String>::new();
        let root = temp_dir("scan-once");
        let calls = AtomicUsize::new(0);

        let first = cache.get_or_scan(&root, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec!["alpha".to_string()]
        });
        let second = cache.get_or_scan(&root, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec!["beta".to_string()]
        });

        assert_eq!(first, vec!["alpha".to_string()]);
        assert_eq!(second, vec!["alpha".to_string()]);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalidate_forces_rescan_on_next_read() {
        let cache = SkillCache::<String>::new();
        let root = temp_dir("invalidate");
        let calls = AtomicUsize::new(0);

        let first = cache.get_or_scan(&root, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec!["old".to_string()]
        });
        assert_eq!(first, vec!["old".to_string()]);

        cache.invalidate(&root);
        let second = cache.get_or_scan(&root, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec!["new".to_string()]
        });
        assert_eq!(second, vec!["new".to_string()]);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let third = cache.get_or_scan(&root, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec!["never".to_string()]
        });
        assert_eq!(third, vec!["new".to_string()]);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn is_dirty_reports_never_scanned_and_invalidated() {
        let cache = SkillCache::<u32>::new();
        let root = temp_dir("dirty");

        assert!(cache.is_dirty(&root), "never scanned root must be dirty");
        let _ = cache.get_or_scan(&root, || vec![1]);
        assert!(!cache.is_dirty(&root));

        cache.invalidate(&root);
        assert!(cache.is_dirty(&root), "invalidated root must be dirty");
        let _ = cache.get_or_scan(&root, || vec![2]);
        assert!(!cache.is_dirty(&root));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn watcher_marks_root_dirty_on_skill_creation() {
        let app = tauri::test::mock_app();
        let root = temp_dir("watcher-create");
        let cache = Arc::new(SkillCache::<u32>::new());
        let handle = start_skill_watchers(vec![root.clone()], cache.clone(), app.handle());
        assert!(cache.is_dirty(&root));

        // Make the root clean first so the invalidation below is observable.
        let _ = cache.get_or_scan(&root, || vec![0]);
        assert!(!cache.is_dirty(&root));

        let skill_dir = root.join("new-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "---\nname: new-skill\n---\n")
            .expect("write skill manifest");

        let mut dirty = false;
        for _ in 0..60 {
            if cache.is_dirty(&root) {
                dirty = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        drop(handle);
        let _ = fs::remove_dir_all(&root);
        assert!(dirty, "watcher must invalidate on SKILL.md creation");
    }

    #[test]
    fn watcher_ignores_non_markdown_files() {
        let app = tauri::test::mock_app();
        let root = temp_dir("watcher-ignore");
        let cache = Arc::new(SkillCache::<u32>::new());
        let handle = start_skill_watchers(vec![root.clone()], cache.clone(), app.handle());

        let _ = cache.get_or_scan(&root, || vec![0]);
        assert!(!cache.is_dirty(&root));

        fs::write(root.join("notes.txt"), "hello").expect("write txt");
        fs::create_dir_all(root.join("subdir")).expect("create subdir");
        fs::write(root.join("subdir").join("data.bin"), b"\x00").expect("write bin");
        thread::sleep(Duration::from_millis(600));

        drop(handle);
        let _ = fs::remove_dir_all(&root);
        assert!(!cache.is_dirty(&root), "non-markdown events must not invalidate");
    }
}
