//! 应用运行日志：`app_data_dir/logs/nineclaw-YYYY-MM-DD.log`（按自然日滚动）。

use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

const FILE_PREFIX: &str = "nineclaw-";
const READ_MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLogFileMeta {
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLogsOverview {
    pub dir: String,
    pub files: Vec<AppLogFileMeta>,
    pub total_bytes: u64,
}

pub fn logs_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let mut dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    dir.push("logs");
    Ok(dir)
}

fn validate_log_basename(name: &str) -> Result<(), String> {
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("非法文件名".to_string());
    }
    if !name.starts_with(FILE_PREFIX) || !name.ends_with(".log") {
        return Err("不是应用日志文件".to_string());
    }
    Ok(())
}

pub fn list_logs(app: &tauri::AppHandle) -> Result<AppLogsOverview, String> {
    let dir = logs_directory(app)?;
    let dir_str = dir.to_string_lossy().to_string();
    if !dir.is_dir() {
        return Ok(AppLogsOverview {
            dir: dir_str,
            files: vec![],
            total_bytes: 0,
        });
    }
    let mut files: Vec<AppLogFileMeta> = Vec::new();
    let mut total: u64 = 0;
    for entry in fs::read_dir(&dir).map_err(|e| format!("读取日志目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(FILE_PREFIX) || !name.ends_with(".log") {
            continue;
        }
        let meta = entry
            .metadata()
            .map_err(|e| format!("读取文件信息失败: {e}"))?;
        let size = meta.len();
        total = total.saturating_add(size);
        files.push(AppLogFileMeta {
            name,
            size_bytes: size,
        });
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(AppLogsOverview {
        dir: dir_str,
        files,
        total_bytes: total,
    })
}

pub fn read_log_file(app: &tauri::AppHandle, file_name: String) -> Result<String, String> {
    validate_log_basename(&file_name)?;
    let dir = logs_directory(app)?;
    let path = dir.join(&file_name);
    if !path.is_file() {
        return Err("日志文件不存在".to_string());
    }
    let size = fs::metadata(&path)
        .map_err(|e| format!("读取文件信息失败: {e}"))?
        .len();
    if size > READ_MAX_BYTES {
        return Err(format!(
            "日志文件过大（约 {} MB），请使用「导出全部」后用外部编辑器查看。",
            size.saturating_div(1024 * 1024)
        ));
    }
    fs::read_to_string(&path).map_err(|e| format!("读取日志失败: {e}"))
}

pub fn export_logs_to_dir(app: &tauri::AppHandle, dest_dir: String) -> Result<u32, String> {
    let dest = PathBuf::from(dest_dir.trim());
    if !dest.is_dir() {
        return Err("目标不是已存在的目录".to_string());
    }
    let src_dir = match logs_directory(app) {
        Ok(p) => p,
        Err(e) => return Err(e),
    };
    if !src_dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0u32;
    for entry in fs::read_dir(&src_dir).map_err(|e| format!("读取日志目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(FILE_PREFIX) || !name.ends_with(".log") {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        fs::copy(&from, &to).map_err(|e| format!("复制 {name} 失败: {e}"))?;
        count += 1;
    }
    Ok(count)
}

#[tauri::command]
pub fn app_log_open_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = logs_directory(&app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::open_path_in_default_app(&dir)
}

#[tauri::command]
pub fn app_log_list(app: tauri::AppHandle) -> Result<AppLogsOverview, String> {
    list_logs(&app)
}

#[tauri::command]
pub fn app_log_read(app: tauri::AppHandle, file_name: String) -> Result<String, String> {
    read_log_file(&app, file_name)
}

#[tauri::command]
pub fn app_log_export_all(app: tauri::AppHandle, dest_dir: String) -> Result<u32, String> {
    export_logs_to_dir(&app, dest_dir)
}
