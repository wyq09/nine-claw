use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const MCP_SETTINGS_KEY: &str = "mcp_settings_v1";
const RUNTIME_MCP_SETTINGS_FILE: &str = "nineclaw-mcp-settings.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportType {
    Stdio,
    #[default]
    StreamableHttp,
    Sse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub transport: McpTransportType,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpSettings {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

fn default_enabled() -> bool {
    true
}

fn trim_record(mut value: HashMap<String, String>) -> HashMap<String, String> {
    value.retain(|key, entry| !key.trim().is_empty() && !entry.trim().is_empty());
    value
        .into_iter()
        .map(|(key, entry)| (key.trim().to_string(), entry.trim().to_string()))
        .collect()
}

fn normalize_server(mut server: McpServerConfig) -> McpServerConfig {
    server.id = server.id.trim().to_string();
    server.name = server.name.trim().to_string();
    server.command = server.command.trim().to_string();
    server.args = server
        .args
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    server.env = trim_record(server.env);
    server.cwd = server.cwd.trim().to_string();
    server.url = server.url.trim().to_string();
    server.headers = trim_record(server.headers);
    server
}

fn validate_server(server: &McpServerConfig) -> Result<(), String> {
    if server.id.is_empty() {
        return Err("MCP server id 不能为空".to_string());
    }

    match server.transport {
        McpTransportType::Stdio => {
            if server.command.is_empty() {
                return Err(format!("MCP server `{}` 的 command 不能为空", server.id));
            }
        }
        McpTransportType::StreamableHttp | McpTransportType::Sse => {
            if server.url.is_empty() {
                return Err(format!("MCP server `{}` 的 url 不能为空", server.id));
            }
            let parsed = reqwest::Url::parse(&server.url)
                .map_err(|error| format!("MCP server `{}` 的 url 非法: {error}", server.id))?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err(format!(
                    "MCP server `{}` 的 url 只支持 http / https",
                    server.id
                ));
            }
        }
    }

    Ok(())
}

fn normalize_settings(settings: McpSettings) -> McpSettings {
    let mut seen = HashSet::new();
    let mut servers = Vec::new();
    for server in settings.servers {
        let normalized = normalize_server(server);
        if normalized.id.is_empty() || !seen.insert(normalized.id.clone()) {
            continue;
        }
        servers.push(normalized);
    }
    McpSettings { servers }
}

fn validate_settings(settings: &McpSettings) -> Result<(), String> {
    for server in &settings.servers {
        validate_server(server)?;
    }
    Ok(())
}

fn load_mcp_settings_from_conn(conn: &Connection) -> Result<McpSettings, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![MCP_SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 MCP 设置失败: {error}"))?;
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(McpSettings::default());
    };
    let settings = serde_json::from_str::<McpSettings>(&raw)
        .map_err(|error| format!("解析 MCP 设置失败: {error}"))?;
    let normalized = normalize_settings(settings);
    validate_settings(&normalized)?;
    Ok(normalized)
}

fn save_mcp_settings_to_conn(conn: &Connection, settings: &McpSettings) -> Result<(), String> {
    let json =
        serde_json::to_string(settings).map_err(|error| format!("序列化 MCP 设置失败: {error}"))?;
    let now = crate::chrono_like_timestamp();
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![MCP_SETTINGS_KEY, json, now],
    )
    .map_err(|error| format!("保存 MCP 设置失败: {error}"))?;
    Ok(())
}

pub fn load_mcp_settings(app: &AppHandle) -> Result<McpSettings, String> {
    let conn = crate::open_history_db(app)?;
    load_mcp_settings_from_conn(&conn)
}

pub fn save_mcp_settings(app: &AppHandle, settings: &McpSettings) -> Result<McpSettings, String> {
    let normalized = normalize_settings(settings.clone());
    validate_settings(&normalized)?;
    let conn = crate::open_history_db(app)?;
    save_mcp_settings_to_conn(&conn, &normalized)?;
    Ok(normalized)
}

/// Insert or update the given servers (matched by id) into the settings.
/// Returns the normalized + validated merged settings.
pub fn upsert_servers(
    settings: &McpSettings,
    incoming: Vec<McpServerConfig>,
) -> Result<McpSettings, String> {
    let mut servers = settings.servers.clone();
    for raw in incoming {
        let normalized = normalize_server(raw);
        if normalized.id.is_empty() {
            return Err("MCP server id 不能为空".to_string());
        }
        validate_server(&normalized)?;
        if let Some(existing) = servers.iter_mut().find(|server| server.id == normalized.id) {
            *existing = normalized;
        } else {
            servers.push(normalized);
        }
    }
    let merged = normalize_settings(McpSettings { servers });
    validate_settings(&merged)?;
    Ok(merged)
}

/// Remove a server by id. Returns the new settings and whether a server was removed.
pub fn remove_server(settings: &McpSettings, server_id: &str) -> (McpSettings, bool) {
    let target = server_id.trim();
    let before = settings.servers.len();
    let servers: Vec<McpServerConfig> = settings
        .servers
        .iter()
        .filter(|server| server.id != target)
        .cloned()
        .collect();
    let removed = servers.len() != before;
    (McpSettings { servers }, removed)
}

/// Toggle the `enabled` flag of a server by id. Returns the new settings and whether the server was found.
pub fn set_server_enabled(
    settings: &McpSettings,
    server_id: &str,
    enabled: bool,
) -> (McpSettings, bool) {
    let target = server_id.trim();
    let mut servers = settings.servers.clone();
    let mut found = false;
    for server in servers.iter_mut() {
        if server.id == target {
            server.enabled = enabled;
            found = true;
        }
    }
    (McpSettings { servers }, found)
}

pub fn runtime_settings_snapshot(settings: &McpSettings) -> Value {
    let mut servers = Map::new();
    for server in &settings.servers {
        servers.insert(
            server.id.clone(),
            json!({
                "name": server.name,
                "transport": server.transport,
                "enabled": server.enabled,
                "command": server.command,
                "args": server.args,
                "env": server.env,
                "cwd": server.cwd,
                "url": server.url,
                "headers": server.headers,
            }),
        );
    }
    Value::Object(
        [("mcpServers".to_string(), Value::Object(servers))]
            .into_iter()
            .collect(),
    )
}

pub fn write_runtime_settings_snapshot(
    runtime_dir: &Path,
    settings: &McpSettings,
) -> Result<PathBuf, String> {
    fs::create_dir_all(runtime_dir).map_err(|error| {
        format!(
            "创建 MCP runtime 目录失败 {}: {error}",
            runtime_dir.display()
        )
    })?;
    let path = runtime_dir.join(RUNTIME_MCP_SETTINGS_FILE);
    let json = serde_json::to_string_pretty(&runtime_settings_snapshot(settings))
        .map_err(|error| format!("序列化 MCP runtime 配置失败: {error}"))?;
    fs::write(&path, json)
        .map_err(|error| format!("写入 MCP runtime 配置失败 {}: {error}", path.display()))?;
    Ok(path)
}

/// Returns the fixed global snapshot path: `{app_data_dir}/nineclaw-mcp-settings.json`.
/// All sessions inject this same path as `NINECLAW_MCP_CONFIG_FILE`, so saving settings
/// from the UI or via mcp_config takes effect immediately without a session restart.
pub fn global_snapshot_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取 app data 目录失败: {error}"))?;
    Ok(dir.join(RUNTIME_MCP_SETTINGS_FILE))
}

/// Write the global snapshot file and return its path. Creates the directory if needed.
pub fn write_global_snapshot(app: &AppHandle, settings: &McpSettings) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取 app data 目录失败: {error}"))?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("创建 app data 目录失败 {}: {error}", dir.display()))?;
    let path = dir.join(RUNTIME_MCP_SETTINGS_FILE);
    let json = serde_json::to_string_pretty(&runtime_settings_snapshot(settings))
        .map_err(|error| format!("序列化 MCP runtime 配置失败: {error}"))?;
    fs::write(&path, json)
        .map_err(|error| format!("写入全局 MCP 配置快照失败 {}: {error}", path.display()))?;
    Ok(path)
}

/// Save settings to DB and update the global snapshot atomically.
pub fn save_mcp_settings_and_snapshot(
    app: &AppHandle,
    settings: &McpSettings,
) -> Result<McpSettings, String> {
    let saved = save_mcp_settings(app, settings)?;
    if let Err(error) = write_global_snapshot(app, &saved) {
        log::warn!("写入全局 MCP 快照失败（配置已保存到 DB）: {error}");
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_settings_round_trip() {
        let conn = crate::storage::db::open_in_memory().expect("open in-memory db");
        let settings = McpSettings {
            servers: vec![McpServerConfig {
                id: "miview".into(),
                name: "MiView".into(),
                transport: McpTransportType::StreamableHttp,
                enabled: true,
                command: String::new(),
                args: Vec::new(),
                env: HashMap::new(),
                cwd: String::new(),
                url: "http://127.0.0.1:25424/mcp".into(),
                headers: HashMap::from([(
                    "Authorization".to_string(),
                    "Bearer test_token".to_string(),
                )]),
            }],
        };

        save_mcp_settings_to_conn(&conn, &settings).expect("save settings");
        let loaded = load_mcp_settings_from_conn(&conn).expect("load settings");

        assert_eq!(loaded, settings);
    }

    #[test]
    fn normalizes_and_dedupes_servers() {
        let settings = normalize_settings(McpSettings {
            servers: vec![
                McpServerConfig {
                    id: "  alpha  ".into(),
                    name: "  Alpha  ".into(),
                    transport: McpTransportType::Stdio,
                    enabled: true,
                    command: "  npx  ".into(),
                    args: vec!["  -y ".into(), "".into()],
                    env: HashMap::from([(" TOKEN ".into(), " abc ".into())]),
                    cwd: "  /tmp  ".into(),
                    url: String::new(),
                    headers: HashMap::new(),
                },
                McpServerConfig {
                    id: "alpha".into(),
                    ..McpServerConfig::default()
                },
            ],
        });

        assert_eq!(settings.servers.len(), 1);
        assert_eq!(settings.servers[0].id, "alpha");
        assert_eq!(settings.servers[0].name, "Alpha");
        assert_eq!(settings.servers[0].command, "npx");
        assert_eq!(settings.servers[0].args, vec!["-y"]);
        assert_eq!(settings.servers[0].env.get("TOKEN").map(String::as_str), Some("abc"));
        assert_eq!(settings.servers[0].cwd, "/tmp");
    }

    fn http_server(id: &str, url: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            transport: McpTransportType::StreamableHttp,
            enabled: true,
            url: url.into(),
            ..McpServerConfig::default()
        }
    }

    #[test]
    fn upsert_adds_new_and_updates_existing() {
        let base = McpSettings {
            servers: vec![http_server("alpha", "https://a.example/mcp")],
        };

        let merged = upsert_servers(
            &base,
            vec![
                http_server("beta", "https://b.example/mcp"),
                // Update alpha's url
                http_server("alpha", "https://a2.example/mcp"),
            ],
        )
        .expect("upsert");

        assert_eq!(merged.servers.len(), 2);
        let alpha = merged.servers.iter().find(|s| s.id == "alpha").unwrap();
        assert_eq!(alpha.url, "https://a2.example/mcp");
        assert!(merged.servers.iter().any(|s| s.id == "beta"));
    }

    #[test]
    fn upsert_rejects_invalid_server() {
        let base = McpSettings::default();
        let err = upsert_servers(
            &base,
            vec![McpServerConfig {
                id: "broken".into(),
                transport: McpTransportType::Sse,
                url: String::new(),
                ..McpServerConfig::default()
            }],
        )
        .unwrap_err();
        assert!(err.contains("url"));
    }

    #[test]
    fn remove_server_drops_matching_id() {
        let base = McpSettings {
            servers: vec![
                http_server("alpha", "https://a.example/mcp"),
                http_server("beta", "https://b.example/mcp"),
            ],
        };
        let (after, removed) = remove_server(&base, "alpha");
        assert!(removed);
        assert_eq!(after.servers.len(), 1);
        assert_eq!(after.servers[0].id, "beta");

        let (after2, removed2) = remove_server(&after, "ghost");
        assert!(!removed2);
        assert_eq!(after2.servers.len(), 1);
    }

    #[test]
    fn set_server_enabled_toggles_flag() {
        let base = McpSettings {
            servers: vec![http_server("alpha", "https://a.example/mcp")],
        };
        let (after, found) = set_server_enabled(&base, "alpha", false);
        assert!(found);
        assert!(!after.servers[0].enabled);

        let (_after2, missing) = set_server_enabled(&after, "ghost", true);
        assert!(!missing);
    }

    #[test]
    fn writes_runtime_snapshot_in_standard_shape() {
        let runtime_dir = std::env::temp_dir().join("nineclaw-mcp-settings-test");
        let settings = McpSettings {
            servers: vec![McpServerConfig {
                id: "miview".into(),
                name: "MiView".into(),
                transport: McpTransportType::StreamableHttp,
                enabled: true,
                url: "http://127.0.0.1:25424/mcp".into(),
                headers: HashMap::from([(
                    "Authorization".to_string(),
                    "Bearer token".to_string(),
                )]),
                ..McpServerConfig::default()
            }],
        };

        let snapshot_path =
            write_runtime_settings_snapshot(&runtime_dir, &settings).expect("write snapshot");
        let raw = fs::read_to_string(&snapshot_path).expect("read snapshot");
        let parsed: Value = serde_json::from_str(&raw).expect("parse snapshot");

        assert_eq!(
            parsed["mcpServers"]["miview"]["url"].as_str(),
            Some("http://127.0.0.1:25424/mcp")
        );
        assert_eq!(
            parsed["mcpServers"]["miview"]["transport"].as_str(),
            Some("streamable_http")
        );

        let _ = fs::remove_file(snapshot_path);
        let _ = fs::remove_dir_all(runtime_dir);
    }
}
