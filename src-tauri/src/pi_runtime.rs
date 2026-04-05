use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

const PI_RUNTIME_RESOURCE_DIR: &str = "pi-runtime";

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

    let mut candidates = development_runtime_candidates();
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

    candidates
}

pub(crate) fn resolve_bundled_pi_executable(app: &AppHandle) -> Option<PathBuf> {
    bundled_runtime_candidates(app)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

pub(crate) fn resolve_pi_executable(app: &AppHandle) -> Option<PiRuntimeLocation> {
    prime_runtime_path_for_platform();

    if let Some(executable) = resolve_bundled_pi_executable(app) {
        if let Some(parent) = executable.parent() {
            prepend_to_path(parent);
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

pub(crate) fn require_pi_executable(app: &AppHandle) -> Result<PathBuf, String> {
    resolve_pi_executable(app)
        .map(|location| location.executable)
        .ok_or_else(|| "未找到 pi 运行时。请先执行 `npm install` 并运行 `npm run prepare:pi-runtime` 生成内置运行时，或者确认系统 `pi` 已在 PATH 中可用。".to_string())
}

pub(crate) fn create_pi_command(app: &AppHandle) -> Result<Command, String> {
    let location = resolve_pi_executable(app).ok_or_else(|| {
        "未找到 pi 运行时。请先执行 `npm install` 并运行 `npm run prepare:pi-runtime` 生成内置运行时，或者确认系统 `pi` 已在 PATH 中可用。".to_string()
    })?;
    let mut command = Command::new(&location.executable);
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
    Ok(command)
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
