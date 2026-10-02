//! Jev 工具路由设置（sqlite KV 存储，模式参考 embedding_settings）。
//!
//! Storage: `app_state` KV via `history_app_state::storage_conn`, key
//! `tool_router_settings_v1`. Defaults are the safe legacy behavior — tool
//! routing only activates when the user explicitly switches mode to `jev`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::tool_router::policy::{DEFAULT_JEV_MODEL, DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS};

const TOOL_ROUTER_SETTINGS_KEY: &str = "tool_router_settings_v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolRouterMode {
    #[default]
    Legacy,
    Jev,
}

impl ToolRouterMode {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            ToolRouterMode::Legacy => "legacy",
            ToolRouterMode::Jev => "jev",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolRouterSettings {
    /// `legacy` keeps the current behavior (safe default); `jev` enables the
    /// per-turn AI tool-selection layer.
    #[serde(default)]
    pub(crate) mode: ToolRouterMode,
    #[serde(default)]
    pub(crate) api_key: String,
    /// Pinned Jev model; empty normalizes to `jev-1.13.0`, never
    /// `jev-latest`.
    #[serde(default = "default_model")]
    pub(crate) model: String,
    /// Overall round-trip budget in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub(crate) timeout_ms: u64,
}

fn default_model() -> String {
    DEFAULT_JEV_MODEL.to_string()
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

impl Default for ToolRouterSettings {
    fn default() -> Self {
        Self {
            mode: ToolRouterMode::Legacy,
            api_key: String::new(),
            model: default_model(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

/// Trim + clamp user input before persisting.
pub(crate) fn normalize_settings(mut settings: ToolRouterSettings) -> ToolRouterSettings {
    settings.api_key = settings.api_key.trim().to_string();
    settings.model = settings.model.trim().to_string();
    if settings.model.is_empty() {
        settings.model = default_model();
    }
    if settings.timeout_ms == 0 {
        settings.timeout_ms = default_timeout_ms();
    }
    settings.timeout_ms = settings.timeout_ms.min(MAX_TIMEOUT_MS);
    settings
}

pub(crate) fn load_settings_from_conn(
    conn: &Connection,
) -> Result<ToolRouterSettings, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![TOOL_ROUTER_SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 tool router 设置失败: {error}"))?;
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(ToolRouterSettings::default());
    };
    serde_json::from_str::<ToolRouterSettings>(&raw)
        .map(normalize_settings)
        .map_err(|error| format!("解析 tool router 设置失败: {error}"))
}

pub(crate) fn save_settings_to_conn(
    conn: &Connection,
    settings: &ToolRouterSettings,
) -> Result<(), String> {
    let json = serde_json::to_string(settings)
        .map_err(|error| format!("序列化 tool router 设置失败: {error}"))?;
    let now = crate::chrono_like_timestamp();
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![TOOL_ROUTER_SETTINGS_KEY, json, now],
    )
    .map_err(|error| format!("保存 tool router 设置失败: {error}"))?;
    Ok(())
}

pub(crate) fn load_tool_router_settings(
    app: &AppHandle,
) -> Result<ToolRouterSettings, String> {
    let conn = crate::storage_conn(app)?;
    load_settings_from_conn(&conn)
}

pub(crate) fn save_tool_router_settings(
    app: &AppHandle,
    settings: &ToolRouterSettings,
) -> Result<ToolRouterSettings, String> {
    let normalized = normalize_settings(settings.clone());
    let conn = crate::storage_conn(app)?;
    save_settings_to_conn(&conn, &normalized)?;
    Ok(normalized)
}

#[tauri::command]
pub(crate) async fn load_tool_router_settings_command(
    app: AppHandle,
) -> Result<ToolRouterSettings, String> {
    load_tool_router_settings(&app)
}

#[tauri::command]
pub(crate) async fn save_tool_router_settings_command(
    app: AppHandle,
    settings: ToolRouterSettings,
) -> Result<ToolRouterSettings, String> {
    save_tool_router_settings(&app, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    #[test]
    fn defaults_are_safe_legacy_with_pinned_model() {
        let settings = ToolRouterSettings::default();
        assert_eq!(settings.mode, ToolRouterMode::Legacy);
        assert_eq!(settings.model, "jev-1.13.0");
        assert_eq!(settings.timeout_ms, 2000);
        assert!(settings.api_key.is_empty());
    }

    #[test]
    fn deserializes_missing_fields_to_defaults() {
        let loaded: ToolRouterSettings = serde_json::from_str("{}").expect("parse empty");
        assert_eq!(loaded, ToolRouterSettings::default());

        let partial: ToolRouterSettings =
            serde_json::from_str(r#"{"mode":"jev"}"#).expect("parse partial");
        assert_eq!(partial.mode, ToolRouterMode::Jev);
        assert_eq!(partial.model, "jev-1.13.0");
        assert_eq!(partial.timeout_ms, 2000);
    }

    #[test]
    fn serializes_camel_case_for_frontend() {
        let json = serde_json::to_value(ToolRouterSettings::default()).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "mode": "legacy",
                "apiKey": "",
                "model": "jev-1.13.0",
                "timeoutMs": 2000
            })
        );
    }

    #[test]
    fn normalizes_blank_and_extreme_values() {
        let normalized = normalize_settings(ToolRouterSettings {
            mode: ToolRouterMode::Jev,
            api_key: " sk-abc ".to_string(),
            model: "  ".to_string(),
            timeout_ms: 0,
        });
        assert_eq!(normalized.api_key, "sk-abc");
        assert_eq!(normalized.model, "jev-1.13.0");
        assert_eq!(normalized.timeout_ms, 2000);

        let clamped = normalize_settings(ToolRouterSettings {
            mode: ToolRouterMode::Jev,
            api_key: String::new(),
            model: "jev-1.13.0".to_string(),
            timeout_ms: 9_999_999,
        });
        assert_eq!(clamped.timeout_ms, MAX_TIMEOUT_MS);
    }

    #[test]
    fn round_trips_through_storage_kv() {
        let conn = open_in_memory().expect("open memory db");
        let settings = ToolRouterSettings {
            mode: ToolRouterMode::Jev,
            api_key: "sk-test".to_string(),
            model: "jev-1.13.0".to_string(),
            timeout_ms: 1500,
        };
        save_settings_to_conn(&conn, &settings).expect("save tool router settings");
        let loaded = load_settings_from_conn(&conn).expect("load tool router settings");
        assert_eq!(loaded, settings);
    }

    #[test]
    fn missing_row_returns_defaults_without_error() {
        let conn = open_in_memory().expect("open memory db");
        let loaded = load_settings_from_conn(&conn).expect("load tool router settings");
        assert_eq!(loaded, ToolRouterSettings::default());
    }
}
