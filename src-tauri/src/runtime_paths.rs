//! Runtime files that contain conversation/model data (PI session JSONL,
//! multimodal summaries, provider runtime config).
//!
//! Security invariants:
//! - production files live under the per-user app data directory, never in
//!   the shared system temp directory with predictable names;
//! - directories are created `0700` and files are opened `0600` on Unix;
//! - symlinks are rejected before a runtime file is created or overwritten;
//! - legacy `/tmp` session files are copied into the private directory once
//!   and left in place for backward-compatible cleanup.

use crate::app_constants::PI_RUNTIME_DIR_NAME;
use crate::managed_runtime::injected_app_handle;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tauri::Manager;

const RUNTIME_DATA_DIR: &str = "nineclaw-runtime";
const PI_SESSIONS_DIR: &str = "pi-sessions";

fn app_data_dir() -> Option<PathBuf> {
    injected_app_handle()
        .and_then(|app| app.path().app_data_dir().ok())
        .filter(|path| !path.as_os_str().is_empty())
}

fn runtime_data_base() -> PathBuf {
    app_data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(RUNTIME_DATA_DIR)
}

pub(crate) fn pi_sessions_dir() -> PathBuf {
    runtime_data_base().join(PI_SESSIONS_DIR)
}

pub(crate) fn legacy_session_dir() -> PathBuf {
    std::env::temp_dir()
}

pub(crate) fn legacy_session_file_path(prefix: &str, key: &str) -> PathBuf {
    legacy_session_dir().join(format!("{prefix}{key}.jsonl"))
}

pub(crate) fn pi_runtime_dir() -> PathBuf {
    runtime_data_base().join(PI_RUNTIME_DIR_NAME)
}

pub(crate) fn session_file_path(prefix: &str, key: &str) -> PathBuf {
    pi_sessions_dir().join(format!("{prefix}{key}.jsonl"))
}

pub(crate) fn ephemeral_session_file_path(prefix: &str, key: &str) -> PathBuf {
    pi_sessions_dir().join(format!(
        "{prefix}{key}-media-{}.jsonl",
        uuid::Uuid::new_v4().simple()
    ))
}

pub(crate) fn summary_file_path(prefix: &str, key: &str) -> PathBuf {
    pi_sessions_dir().join(format!("{prefix}{key}.json"))
}

fn not_found(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "拒绝写入符号链接路径，可能存在链接攻击: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if not_found(&error) => Ok(()),
        Err(error) => Err(format!("检查路径失败 {}: {error}", path.display())),
    }
}

fn set_private_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if path.is_dir() { 0o700 } else { 0o600 }),
        )
        .map_err(|error| format!("设置私有权限失败 {}: {error}", path.display()))?;
    }
    Ok(())
}

/// Create `path` as a directory and make sure it stays a real directory with
/// owner-only permissions. Existing directories are re-chmod'ed so upgrades
/// from the old `/tmp` era converge to the secure mode.
///
/// Callers must only pass directories this app owns; chmod'ing shared
/// directories like the system temp dir fails on macOS (`EPERM`).
pub(crate) fn ensure_private_dir(path: &Path) -> Result<(), String> {
    reject_symlink(path)?;
    fs::create_dir_all(path)
        .map_err(|error| format!("创建私有运行目录失败 {}: {error}", path.display()))?;
    reject_symlink(path)?;
    if !path.is_dir() {
        return Err(format!("私有运行路径不是目录: {}", path.display()));
    }
    set_private_permissions(path)
}

/// Prepare a parent directory for a runtime file. Unlike
/// [`ensure_private_dir`], directories outside our own runtime tree (e.g.
/// the shared system temp dir) are created but never re-chmod'ed: macOS
/// refuses to chmod shared temp dirs, and they are not ours to tighten.
fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    reject_symlink(path)?;
    fs::create_dir_all(path)
        .map_err(|error| format!("创建私有运行目录失败 {}: {error}", path.display()))?;
    reject_symlink(path)?;
    if !path.is_dir() {
        return Err(format!("私有运行路径不是目录: {}", path.display()));
    }
    if path.starts_with(runtime_data_base()) {
        set_private_permissions(path)?;
    }
    Ok(())
}

fn open_private_file(path: &Path, create_new: bool) -> Result<fs::File, String> {
    if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() {
            return Err(format!("运行时文件缺少父目录: {}", path.display()));
        }
        ensure_parent_dir(parent)?;
    }
    reject_symlink(path)?;

    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(create_new)
        .create(!create_new);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|error| format!("打开私有运行文件失败 {}: {error}", path.display()))
}

/// Make sure `path` exists and is a regular owner-only file. Used for PI
/// session files, which the PI process later opens and appends to.
pub(crate) fn ensure_private_file(path: &Path) -> Result<(), String> {
    let file = open_private_file(path, false)?;
    drop(file);
    set_private_permissions(path)
}

/// Atomically create a brand-new private file (for ephemeral sessions). The
/// caller generates a random filename before calling this.
pub(crate) fn create_private_file(path: &Path) -> Result<(), String> {
    let file = open_private_file(path, true)?;
    drop(file);
    set_private_permissions(path)
}

/// Write a complete runtime file with owner-only permissions. Existing files
/// are truncated and re-chmod'ed.
pub(crate) fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    reject_symlink(path)?;
    if let Some(parent) = path.parent() {
        ensure_parent_dir(parent)?;
    }
    let mut file = OpenOptions::new();
    file.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.mode(0o600);
    }
    let mut file = file
        .open(path)
        .map_err(|error| format!("写入私有运行文件失败 {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("写入私有运行文件失败 {}: {error}", path.display()))?;
    file.flush()
        .map_err(|error| format!("刷新私有运行文件失败 {}: {error}", path.display()))?;
    drop(file);
    set_private_permissions(path)
}

/// Ensure the persistent PI session file exists with `0600`, migrating the
/// newest matching legacy `/tmp` file when the private path does not exist yet.
pub(crate) fn prepare_pi_session_file(path: &Path, legacy_paths: &[PathBuf]) -> Result<(), String> {
    if path.exists() {
        return ensure_private_file(path);
    }

    for legacy_path in legacy_paths {
        reject_symlink(legacy_path)?;
        if !legacy_path.exists() {
            continue;
        }
        let bytes = fs::read(legacy_path)
            .map_err(|error| format!("迁移旧 session 失败 {}: {error}", legacy_path.display()))?;
        write_private_file(path, &bytes)?;
        return Ok(());
    }

    ensure_private_file(path)
}

/// Ensure the PI session file exists and is private. Ephemeral sessions get a
/// brand-new empty file (`create_new = true`); persistent sessions fall back
/// to the legacy `/tmp` file built from `legacy_prefix + legacy_key` when the
/// private file is missing.
pub(crate) fn prepare_or_create_pi_session_file(
    path: &Path,
    legacy_prefix: &str,
    legacy_key: &str,
    create_new: bool,
) -> Result<(), String> {
    if create_new {
        create_private_file(path)
    } else {
        prepare_pi_session_file(path, &[legacy_session_file_path(legacy_prefix, legacy_key)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-runtime-paths-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path).expect("create temp test dir");
        path
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[cfg(unix)]
    #[test]
    fn private_dir_is_0700() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir();
        let dir = root.join("private");
        ensure_private_dir(&dir).expect("ensure dir");
        let mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        cleanup(&root);
    }

    #[cfg(unix)]
    #[test]
    fn private_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir();
        let file = root.join("private").join("session.jsonl");
        write_private_file(&file, b"{}\n").expect("write private file");
        let mode = fs::metadata(&file).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        cleanup(&root);
    }

    #[test]
    fn prepare_session_migrates_legacy_content_once() {
        let root = temp_dir();
        let private_path = root.join("sessions").join("session.jsonl");
        let legacy_path = root.join("legacy-session.jsonl");
        fs::write(&legacy_path, b"{\"legacy\":true}\n").expect("write legacy");

        prepare_pi_session_file(&private_path, std::slice::from_ref(&legacy_path)).expect("prepare session");
        assert_eq!(
            fs::read_to_string(&private_path).expect("read private"),
            "{\"legacy\":true}\n"
        );

        fs::write(&private_path, b"{\"current\":true}\n").expect("rewrite private");
        fs::write(&legacy_path, b"{\"legacy\":false}\n").expect("rewrite legacy");
        prepare_pi_session_file(&private_path, &[legacy_path]).expect("prepare existing session");
        assert_eq!(
            fs::read_to_string(&private_path).expect("read private again"),
            "{\"current\":true}\n"
        );
        cleanup(&root);
    }

    #[test]
    fn prepare_session_creates_empty_file_when_no_legacy() {
        let root = temp_dir();
        let private_path = root.join("sessions").join("session.jsonl");
        prepare_pi_session_file(&private_path, &[]).expect("prepare session");
        assert!(private_path.is_file());
        cleanup(&root);
    }

    #[cfg(unix)]
    #[test]
    fn write_private_file_rejects_symlink_target() {
        use std::os::unix::fs::symlink;
        let root = temp_dir();
        let target = root.join("target.txt");
        fs::write(&target, b"target").expect("write target");
        let link = root.join("link.txt");
        symlink(&target, &link).expect("create symlink");

        let error = write_private_file(&link, b"boom").expect_err("must reject symlink");
        assert!(error.contains("符号链接"), "{error}");
        assert_eq!(fs::read_to_string(&target).expect("read target"), "target");
        cleanup(&root);
    }

    #[test]
    fn write_private_file_under_shared_temp_dir_never_chmods_it() {
        // Regression: sanitizing a legacy session writes into the shared
        // temp dir; chmod'ing that dir fails with EPERM on macOS.
        let shared_temp = std::env::temp_dir();
        let file = shared_temp.join(format!(
            "nineclaw-shared-temp-write-test-{}.jsonl",
            uuid::Uuid::new_v4().simple()
        ));
        write_private_file(&file, b"{}\n").expect("write under shared temp dir");
        assert_eq!(
            fs::read_to_string(&file).expect("read back"),
            "{}\n",
            "file content must survive"
        );
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn legacy_session_file_path_uses_shared_temp_dir() {
        let path = legacy_session_file_path("nineclaw-bot-session-", "abc");
        assert_eq!(
            path,
            std::env::temp_dir().join("nineclaw-bot-session-abc.jsonl")
        );
    }

    #[test]
    fn prepare_or_create_creates_fresh_ephemeral_file() {
        let root = temp_dir();
        let path = root.join("sessions").join("fresh.jsonl");
        prepare_or_create_pi_session_file(&path, "legacy-prefix-", "abc", true)
            .expect("create fresh");
        assert!(path.is_file());
        assert_eq!(fs::read_to_string(&path).expect("read fresh"), "");
        cleanup(&root);
    }

    #[test]
    fn prepare_or_create_migrates_legacy_when_not_fresh() {
        let root = temp_dir();
        // Legacy files always live in the shared temp dir; write one there so
        // the migration source matches what production sees.
        let legacy_path = legacy_session_file_path("legacy-prefix-", "abc");
        let _ = fs::remove_file(&legacy_path);
        fs::write(&legacy_path, b"{\"old\":1}\n").expect("write legacy in shared temp");
        let path = root.join("sessions").join("session.jsonl");
        prepare_or_create_pi_session_file(&path, "legacy-prefix-", "abc", false).expect("prepare");
        assert_eq!(
            fs::read_to_string(&path).expect("read session"),
            "{\"old\":1}\n"
        );
        let _ = fs::remove_file(&legacy_path);
        cleanup(&root);
    }
}
