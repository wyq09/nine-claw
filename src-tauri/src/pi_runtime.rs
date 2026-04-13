use serde::Serialize;
use std::env;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const PI_RUNTIME_RESOURCE_DIR: &str = "pi-runtime";
const PI_RUNTIME_BUNDLE_DIR: &str = "pi-runtime-bundles";
const PI_RUNTIME_EXTRACT_DIR: &str = "pi-runtime-extracted";

/// macOS 上部分 npm 包随附的 `node` 仅约 68KB（thin 包装器），依赖 adhoc 签名的 libnode，会在 Darwin 25+ 被 AMFI SIGKILL。
/// `prepare-pi-runtime.mjs` 从 nodejs.org 下载的官方 `node` 通常为数十 MB，可与 PI、飞书 helper 等共用。
pub(crate) const MIN_NODE_BYTES_PLAUSIBLE_FULL: u64 = 2 * 1024 * 1024;

pub(crate) fn bundled_node_executable_path(resource_root: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        resource_root.join("node.exe")
    } else {
        resource_root.join("node")
    }
}

/// 是否为「完整」Node 可执行体（用于在 macOS 上拒绝 thin stub，避免 SIGKILL）。
pub(crate) fn is_plausible_full_node_binary(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if !cfg!(target_os = "macos") {
        return true;
    }
    fs::metadata(path)
        .map(|m| m.len() >= MIN_NODE_BYTES_PLAUSIBLE_FULL)
        .unwrap_or(false)
}

/// Global flag: repair_runtime_directory() should only run once per process.
static REPAIR_DONE: AtomicBool = AtomicBool::new(false);

fn platform_dir_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn platform_executable_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["pi.exe", "pi.cmd", "pi.bat"]
    } else {
        &["pi"]
    }
}

fn development_runtime_candidates() -> Vec<PathBuf> {
    if !cfg!(debug_assertions) {
        return Vec::new();
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(PI_RUNTIME_RESOURCE_DIR)
        .join(platform_dir_name());

    platform_executable_names()
        .iter()
        .map(|executable_name| root.join(executable_name))
        .collect()
}

fn development_runtime_root() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }

    Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(PI_RUNTIME_RESOURCE_DIR)
            .join(platform_dir_name()),
    )
}

fn is_development_runtime_path(path: &Path) -> bool {
    let Some(root) = development_runtime_root() else {
        return false;
    };

    let canonical_root = fs::canonicalize(&root).unwrap_or(root);
    let canonical_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical_path.starts_with(canonical_root)
}

fn development_runtime_archive_candidates() -> Vec<PathBuf> {
    if !cfg!(debug_assertions) {
        return Vec::new();
    }

    vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(PI_RUNTIME_BUNDLE_DIR)
        .join(format!("{}.tar.gz", platform_dir_name()))]
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PiRuntimeSource {
    Bundled,
    SystemPath,
}

impl PiRuntimeSource {
    fn label(self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::SystemPath => "system_path",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PiRuntimeLocation {
    pub executable: PathBuf,
    pub source: PiRuntimeSource,
    pub resource_root: Option<PathBuf>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDependencyStatus {
    pub platform: String,
    pub node_available: bool,
    pub npm_available: bool,
    pub pi_available: bool,
    pub bundled_pi_available: bool,
    pub bundled_pi_path: Option<String>,
    pub resolved_pi_path: Option<String>,
    pub pi_source: Option<PiRuntimeSource>,
    pub auto_install_attempted: bool,
    pub auto_install_succeeded: bool,
    pub messages: Vec<String>,
}

/// Ensures `ensure_runtime_dependencies_impl` runs at most once per process and returns the same
/// snapshot to the frontend invoke and the startup background thread (avoids concurrent unpack).
static PI_RUNTIME_STATUS_CACHE: Mutex<Option<RuntimeDependencyStatus>> = Mutex::new(None);

pub(crate) fn cached_ensure_runtime_dependencies(app: &AppHandle) -> RuntimeDependencyStatus {
    let mut guard = PI_RUNTIME_STATUS_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(status) = guard.as_ref() {
        return status.clone();
    }
    let status = ensure_runtime_dependencies_impl(app);
    *guard = Some(status.clone());
    status
}

pub(crate) fn resolve_command_path(candidates: &[&str]) -> Option<PathBuf> {
    let resolver = if cfg!(target_os = "windows") {
        ("where", "/")
    } else {
        ("which", "")
    };

    for candidate in candidates {
        let output = Command::new(resolver.0).arg(candidate).output().ok()?;
        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = stdout.lines().map(str::trim).find(|line| !line.is_empty()) {
            return Some(PathBuf::from(first_line));
        }
    }

    None
}

fn prepend_to_path(path: &Path) {
    let Some(path_str) = path.to_str() else {
        return;
    };

    let current = env::var_os("PATH").unwrap_or_default();
    let already_present = env::split_paths(&current).any(|entry| entry == path);
    if already_present {
        return;
    }

    let mut updated = vec![path.to_path_buf()];
    updated.extend(env::split_paths(&current));
    if let Ok(joined) = env::join_paths(updated) {
        env::set_var("PATH", joined);
    } else {
        let mut fallback = path_str.to_string();
        if !current.is_empty() {
            fallback.push(if cfg!(target_os = "windows") {
                ';'
            } else {
                ':'
            });
            fallback.push_str(&current.to_string_lossy());
        }
        env::set_var("PATH", fallback);
    }
}

pub(crate) fn apply_runtime_environment(command: &mut Command, location: &PiRuntimeLocation) {
    if let Some(parent) = location.executable.parent() {
        prepend_to_path(parent);
    }

    if let Some(resource_root) = location.resource_root.as_ref() {
        command.env("NINECLAW_PI_RUNTIME_ROOT", resource_root.as_os_str());
        let mono_root = resource_root.join("pi-mono");
        if mono_root.is_dir() {
            command.env("NINECLAW_PI_MONO_ROOT", mono_root.as_os_str());
        }
    }
}

fn windows_common_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(app_data) = env::var_os("APPDATA") {
        dirs.push(PathBuf::from(app_data).join("npm"));
    }
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        dirs.push(
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("WinGet")
                .join("Links"),
        );
    }
    dirs.push(PathBuf::from(r"C:\Program Files\nodejs"));
    dirs.push(PathBuf::from(r"C:\Program Files (x86)\nodejs"));

    dirs
}

pub(crate) fn prime_runtime_path_for_platform() {
    if cfg!(target_os = "windows") {
        for dir in windows_common_bin_dirs() {
            if dir.exists() {
                prepend_to_path(&dir);
            }
        }
    }
}

fn bundled_runtime_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let Ok(resource_dir) = app.path().resource_dir() else {
        return development_runtime_candidates();
    };

    let platform_dir = platform_dir_name();
    let executable_names = platform_executable_names();

    // Tauri v2 on macOS may nest resources under an extra "resources/" subdirectory:
    //   Contents/Resources/resources/pi-runtime/macos/pi
    // while on other platforms (or dev mode) the path may be flat:
    //   <resource_dir>/pi-runtime/macos/pi
    let search_roots: Vec<PathBuf> = vec![resource_dir.join("resources"), resource_dir.clone()];

    let mut candidates = Vec::new();
    for search_root in &search_roots {
        for executable_name in executable_names {
            candidates.push(
                search_root
                    .join(PI_RUNTIME_RESOURCE_DIR)
                    .join(platform_dir)
                    .join(executable_name),
            );
            candidates.push(
                search_root
                    .join(PI_RUNTIME_RESOURCE_DIR)
                    .join(executable_name),
            );
        }
    }
    candidates.extend(development_runtime_candidates());

    candidates
}

fn bundled_runtime_archive_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let Ok(resource_dir) = app.path().resource_dir() else {
        return development_runtime_archive_candidates();
    };

    let archive_name = format!("{}.tar.gz", platform_dir_name());
    let search_roots: Vec<PathBuf> = vec![resource_dir.join("resources"), resource_dir.clone()];
    let mut candidates = Vec::new();
    for search_root in &search_roots {
        candidates.push(search_root.join(PI_RUNTIME_BUNDLE_DIR).join(&archive_name));
    }
    candidates.extend(development_runtime_archive_candidates());
    candidates
}

fn extraction_state_path(root: &Path) -> PathBuf {
    root.join(".bundle-source")
}

fn desired_runtime_extract_root(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_local_data_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|error| format!("解析本地 pi runtime 目录失败: {error}"))?;
    Ok(base.join(PI_RUNTIME_EXTRACT_DIR).join(platform_dir_name()))
}

pub(crate) fn repair_runtime_directory(root: &Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    // Only run once per process — the repair is expensive (walks entire tree,
    // sets permissions on every file, runs xattr -cr on macOS).
    if REPAIR_DONE.load(Ordering::Acquire) {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("xattr")
            .args(["-cr", root.to_string_lossy().as_ref()])
            .status();
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("读取 pi runtime 路径失败 {}: {error}", path.display()))?;
        if metadata.is_dir() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&path, PermissionsExt::from_mode(0o755));
            }
            for entry in fs::read_dir(&path)
                .map_err(|error| format!("遍历 pi runtime 目录失败 {}: {error}", path.display()))?
            {
                let entry =
                    entry.map_err(|error| format!("读取 pi runtime 目录项失败: {error}"))?;
                stack.push(entry.path());
            }
            continue;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let executable = file_name == "pi"
                || file_name == "node"
                || file_name.ends_with(".dylib")
                || file_name.ends_with(".so")
                || file_name.ends_with(".dll");
            let mode = if executable { 0o755 } else { 0o644 };
            let _ = fs::set_permissions(&path, PermissionsExt::from_mode(mode));
        }
    }

    REPAIR_DONE.store(true, Ordering::Release);
    Ok(())
}

fn ensure_extracted_runtime(app: &AppHandle) -> Result<PathBuf, String> {
    let Some(archive_path) = bundled_runtime_archive_candidates(app)
        .into_iter()
        .find(|candidate| candidate.is_file())
    else {
        return Err("未找到内置 pi runtime 归档".to_string());
    };

    let archive_real = fs::canonicalize(&archive_path).unwrap_or(archive_path.clone());
    let extract_root = desired_runtime_extract_root(app)?;
    let state_path = extraction_state_path(&extract_root);
    let expected_state = archive_real.to_string_lossy().to_string();
    let needs_extract = !extract_root.is_dir()
        || !platform_executable_names()
            .iter()
            .any(|name| extract_root.join(name).is_file())
        || fs::read_to_string(&state_path)
            .ok()
            .map(|value| value.trim().to_string())
            != Some(expected_state.clone());

    if needs_extract {
        if extract_root.exists() {
            fs::remove_dir_all(&extract_root)
                .map_err(|error| format!("清理旧 pi runtime 解包目录失败: {error}"))?;
        }
        fs::create_dir_all(
            extract_root
                .parent()
                .ok_or_else(|| "pi runtime 解包目录无父目录".to_string())?,
        )
        .map_err(|error| format!("创建 pi runtime 解包父目录失败: {error}"))?;

        let file = fs::File::open(&archive_real).map_err(|error| {
            format!(
                "打开 pi runtime 归档失败 {}: {error}",
                archive_real.display()
            )
        })?;
        let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(
                extract_root
                    .parent()
                    .ok_or_else(|| "pi runtime 解包目录无父目录".to_string())?,
            )
            .map_err(|error| format!("解包 pi runtime 归档失败: {error}"))?;
        fs::write(&state_path, expected_state)
            .map_err(|error| format!("写入 pi runtime 解包状态失败: {error}"))?;
    }

    repair_runtime_directory(&extract_root)?;
    Ok(extract_root)
}

pub(crate) fn resolve_bundled_pi_executable(app: &AppHandle) -> Option<PathBuf> {
    if let Some(candidate) = bundled_runtime_candidates(app)
        .into_iter()
        .find(|candidate| candidate.is_file() && !is_development_runtime_path(candidate))
    {
        if let Some(root) = candidate.parent() {
            let _ = repair_runtime_directory(root);
        }
        return Some(candidate);
    }

    // In dev mode, always use the extracted runtime archive instead of the
    // checked-in source tree. The repair step mutates permissions/xattrs and
    // will otherwise touch src-tauri/resources/pi-runtime, which can trigger
    // Tauri dev restart loops.
    let extracted_root = ensure_extracted_runtime(app).ok()?;
    platform_executable_names()
        .iter()
        .map(|executable_name| extracted_root.join(executable_name))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn resolve_pi_executable(app: &AppHandle) -> Option<PiRuntimeLocation> {
    prime_runtime_path_for_platform();

    if let Some(executable) = resolve_bundled_pi_executable(app) {
        if let Some(parent) = executable.parent() {
            prepend_to_path(parent);
            let _ = repair_runtime_directory(parent);
        }
        let resource_root = executable.parent().map(Path::to_path_buf);
        return Some(PiRuntimeLocation {
            executable,
            source: PiRuntimeSource::Bundled,
            resource_root,
        });
    }

    let executable = resolve_command_path(&["pi.cmd", "pi.exe", "pi"])?;
    if let Some(parent) = executable.parent() {
        prepend_to_path(parent);
    }

    Some(PiRuntimeLocation {
        executable,
        source: PiRuntimeSource::SystemPath,
        resource_root: None,
    })
}

/// On macOS, check whether the PI binary carries a quarantine extended-attribute
/// and attempt to clear it automatically.  Returns a human-readable warning if
/// the attribute was present (regardless of whether removal succeeded), or
/// `None` if no quarantine was detected.
#[cfg(target_os = "macos")]
pub(crate) fn clear_macos_quarantine_if_present(executable: &Path) -> Option<String> {
    let path_str = executable.to_string_lossy();
    let xattr_output = Command::new("xattr")
        .args(["-l", &*path_str])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&xattr_output.stdout);
    if !stdout.contains("com.apple.quarantine") {
        return None;
    }

    let removed = Command::new("xattr")
        .args(["-cr", &*path_str])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if removed {
        Some(format!(
            "已自动移除 pi 二进制的 macOS 隔离属性: {}",
            path_str
        ))
    } else {
        Some(format!(
            "检测到 macOS 隔离属性，但自动修复未完成: {}",
            path_str
        ))
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn clear_macos_quarantine_if_present(_executable: &Path) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn repair_runtime_and_report(location: &PiRuntimeLocation) -> Option<String> {
    let mut notes = Vec::new();
    if let Some(root) = location.resource_root.as_ref() {
        if repair_runtime_directory(root).is_ok() {
            notes.push(format!(
                "已校验并修复 pi runtime 目录权限: {}",
                root.display()
            ));
        }
    }
    if let Some(note) = clear_macos_quarantine_if_present(&location.executable) {
        notes.push(note);
    }
    if notes.is_empty() {
        None
    } else {
        Some(notes.join(" | "))
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn repair_runtime_and_report(location: &PiRuntimeLocation) -> Option<String> {
    if let Some(root) = location.resource_root.as_ref() {
        let _ = repair_runtime_directory(root);
    }
    None
}

pub(crate) fn require_pi_runtime_location(app: &AppHandle) -> Result<PiRuntimeLocation, String> {
    resolve_pi_executable(app).ok_or_else(|| {
        "未找到 pi 运行时。请先执行 `npm install` 并运行 `npm run prepare:pi-runtime` 生成内置运行时，或者确认系统 `pi` 已在 PATH 中可用。".to_string()
    })
}

fn install_nodejs_with_winget(messages: &mut Vec<String>) -> bool {
    let winget = resolve_command_path(&["winget"]);
    let Some(winget_path) = winget else {
        messages.push("未找到 winget，无法自动安装 Node.js。".to_string());
        return false;
    };

    messages.push("检测到缺少 npm，尝试通过 winget 安装 Node.js LTS。".to_string());
    match Command::new(winget_path)
        .args([
            "install",
            "--id",
            "OpenJS.NodeJS.LTS",
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .status()
    {
        Ok(status) if status.success() => {
            messages.push("Node.js LTS 安装完成，正在刷新 PATH。".to_string());
            prime_runtime_path_for_platform();
            true
        }
        Ok(status) => {
            messages.push(format!("winget 安装 Node.js 失败，退出码: {status}"));
            false
        }
        Err(error) => {
            messages.push(format!("执行 winget 安装 Node.js 失败: {error}"));
            false
        }
    }
}

fn install_pi_with_npm(messages: &mut Vec<String>) -> bool {
    let npm = resolve_command_path(&["npm.cmd", "npm"]);
    let Some(npm_path) = npm else {
        messages.push("未找到 npm，无法自动安装 pi。".to_string());
        return false;
    };

    messages.push("尝试通过 npm 全局安装 pi 运行时。".to_string());
    match Command::new(npm_path)
        .args(["install", "-g", "@mariozechner/pi-coding-agent"])
        .status()
    {
        Ok(status) if status.success() => {
            prime_runtime_path_for_platform();
            true
        }
        Ok(status) => {
            messages.push(format!("npm 安装 pi 失败，退出码: {status}"));
            false
        }
        Err(error) => {
            messages.push(format!("执行 npm 安装 pi 失败: {error}"));
            false
        }
    }
}

pub(crate) fn ensure_runtime_dependencies_impl(app: &AppHandle) -> RuntimeDependencyStatus {
    prime_runtime_path_for_platform();

    let platform = env::consts::OS.to_string();
    let mut messages = Vec::new();
    let mut node_available = resolve_command_path(&["node.exe", "node"]).is_some();
    let mut npm_available = resolve_command_path(&["npm.cmd", "npm"]).is_some();
    let bundled_pi_path = resolve_bundled_pi_executable(app);
    let bundled_pi_available = bundled_pi_path.is_some();
    let mut auto_install_attempted = false;
    let mut auto_install_succeeded = false;

    let mut resolved_pi = if bundled_pi_available {
        resolve_pi_executable(app)
    } else {
        None
    };

    if resolved_pi.is_none() {
        resolved_pi = resolve_pi_executable(app);
    }

    if resolved_pi.is_none() && cfg!(target_os = "windows") && !bundled_pi_available {
        auto_install_attempted = true;

        if !npm_available && !install_nodejs_with_winget(&mut messages) {
            messages.push("自动安装中止：Node.js/npm 仍不可用。".to_string());
        }

        node_available = resolve_command_path(&["node.exe", "node"]).is_some();
        npm_available = resolve_command_path(&["npm.cmd", "npm"]).is_some();

        if npm_available {
            let _ = install_pi_with_npm(&mut messages);
        }

        resolved_pi = resolve_pi_executable(app);
        auto_install_succeeded = resolved_pi.is_some();
    }

    if let Some(path) = bundled_pi_path.as_ref() {
        messages.push(format!("已检测到应用内置 pi 运行时：{}", path.display()));
    }

    if let Some(location) = resolved_pi.as_ref() {
        messages.push(format!(
            "当前使用的 pi 运行时来源：{} ({})",
            location.source.label(),
            location.executable.display()
        ));
    } else if bundled_pi_available {
        messages.push(
            "已配置内置 pi 目录，但未找到可执行文件。请确认 `pi` 已放入对应平台子目录。"
                .to_string(),
        );
    } else {
        messages.push("当前未检测到可用的 pi 运行时。".to_string());
    }

    RuntimeDependencyStatus {
        platform,
        node_available,
        npm_available,
        pi_available: resolved_pi.is_some(),
        bundled_pi_available,
        bundled_pi_path: bundled_pi_path.map(|path| path.to_string_lossy().to_string()),
        resolved_pi_path: resolved_pi
            .as_ref()
            .map(|location| location.executable.to_string_lossy().to_string()),
        pi_source: resolved_pi.as_ref().map(|location| location.source),
        auto_install_attempted,
        auto_install_succeeded,
        messages,
    }
}
