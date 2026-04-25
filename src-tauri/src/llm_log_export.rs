//! 可选：将已完成的 LLM trace 行镜像追加到用户指定目录（与团队/.debug 并列的导出副本）。
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tauri::Manager;

const STATE_FILE: &str = "llm_log_export_v1.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Stored {
    #[serde(default)]
    path: String,
}

fn export_root() -> &'static Mutex<Option<PathBuf>> {
    static ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    ROOT.get_or_init(|| Mutex::new(None))
}

pub fn init(app: &AppHandle) {
    let state_path = match app.path().app_data_dir() {
        Ok(p) => p.join(STATE_FILE),
        Err(_) => return,
    };
    let Ok(raw) = fs::read_to_string(&state_path) else {
        return;
    };
    let Ok(st) = serde_json::from_str::<Stored>(&raw) else {
        return;
    };
    if st.path.trim().is_empty() {
        return;
    }
    let pb = PathBuf::from(st.path.trim());
    if pb.is_dir() {
        if let Ok(mut g) = export_root().lock() {
            *g = Some(pb);
        }
    }
}

pub fn get_dir() -> Option<String> {
    export_root()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|p| p.to_string_lossy().to_string()))
}

pub fn set_dir(app: &AppHandle, path: Option<String>) -> Result<(), String> {
    let opt = path.as_ref().and_then(|p| {
        let t = p.trim();
        if t.is_empty() {
            None
        } else {
            Some(PathBuf::from(t))
        }
    });
    if let Some(ref pb) = opt {
        if !pb.is_dir() {
            return Err("路径不是已存在的目录".to_string());
        }
    }
    {
        let mut g = export_root().lock().map_err(|e| e.to_string())?;
        *g = opt.clone();
    }
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let state_path = app_data.join(STATE_FILE);
    if let Some(p) = opt {
        fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
        let st = Stored {
            path: p.to_string_lossy().to_string(),
        };
        let json = serde_json::to_string(&st).map_err(|e| e.to_string())?;
        fs::write(&state_path, json).map_err(|e| e.to_string())?;
    } else if state_path.is_file() {
        let _ = fs::remove_file(&state_path);
    }
    Ok(())
}

/// 与 `llm_trace::finalize` 写入的完整 JSON 行相同，按日切片写入用户目录。
pub fn mirror_line(line: &str, ts_ms: i64) {
    let root = match export_root().lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    let Some(root) = root else {
        return;
    };
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(&root)?;
        let dt = chrono::Local
            .timestamp_millis_opt(ts_ms)
            .single()
            .unwrap_or_else(chrono::Local::now);
        let file = root.join(format!("llm-trace-{}.jsonl", dt.format("%Y-%m-%d")));
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    })();
    if let Err(e) = result {
        log::warn!("[llm_log_export] 写入导出目录失败: {e}");
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogPreview {
    pub file: Option<String>,
    pub tail: String,
}

fn is_llm_trace_file(name: &str) -> bool {
    name.starts_with("llm-trace-") && name.ends_with(".jsonl")
}

/// 读取导出目录中最近一日志文件末尾（约 48KB）供设置页预览。
pub fn preview_latest() -> Result<LlmLogPreview, String> {
    let Some(root_str) = get_dir() else {
        return Ok(LlmLogPreview {
            file: None,
            tail: "未设置日志目录。".to_string(),
        });
    };
    let root = Path::new(&root_str);
    if !root.is_dir() {
        return Err("日志目录无效或不可访问".to_string());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(root)
        .map_err(|e| format!("读取目录失败: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(is_llm_trace_file)
                .unwrap_or(false)
        })
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    let Some(path) = files.first() else {
        return Ok(LlmLogPreview {
            file: None,
            tail: "目录中还没有 llm-trace-*.jsonl 文件（完成一次带追踪的 LLM 调用后会出现）。"
                .to_string(),
        });
    };
    let meta = fs::metadata(path).map_err(|e| format!("读取文件信息失败: {e}"))?;
    let len = meta.len();
    const PREVIEW_MAX: u64 = 48 * 1024;
    let tail = if len <= PREVIEW_MAX {
        fs::read_to_string(path).map_err(|e| format!("读取文件失败: {e}"))?
    } else {
        let mut f = File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;
        let start = len.saturating_sub(PREVIEW_MAX);
        f.seek(SeekFrom::Start(start))
            .map_err(|e| format!("定位文件失败: {e}"))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .map_err(|e| format!("读取文件失败: {e}"))?;
        let s = String::from_utf8_lossy(&buf);
        if start > 0 {
            s.find('\n')
                .map(|i| s[i + 1..].to_string())
                .unwrap_or_else(|| s.to_string())
        } else {
            s.to_string()
        }
    };
    Ok(LlmLogPreview {
        file: Some(path.to_string_lossy().to_string()),
        tail,
    })
}
