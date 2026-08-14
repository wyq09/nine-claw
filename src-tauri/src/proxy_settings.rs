use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

const NETWORK_PROXY_SETTINGS_KEY: &str = "network_proxy_settings_v1";
const PROXY_ENV_KEYS: [&str; 6] = [
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct NetworkProxySettings {
    #[serde(default)]
    pub use_system_proxy: bool,
    #[serde(default)]
    pub custom_proxy_url: String,
}


#[derive(Debug, Clone, PartialEq, Eq)]
enum EffectiveProxyMode {
    Disabled,
    System,
    Custom(String),
}

#[derive(Debug, Deserialize)]
struct IpifyResponse {
    ip: String,
}

fn normalize_settings(mut settings: NetworkProxySettings) -> NetworkProxySettings {
    settings.custom_proxy_url = settings.custom_proxy_url.trim().to_string();
    settings
}

fn current_proxy_settings_cell() -> &'static Mutex<NetworkProxySettings> {
    static CURRENT_PROXY_SETTINGS: OnceLock<Mutex<NetworkProxySettings>> = OnceLock::new();
    CURRENT_PROXY_SETTINGS.get_or_init(|| Mutex::new(NetworkProxySettings::default()))
}

fn original_proxy_env_snapshot() -> &'static HashMap<String, Option<String>> {
    static ORIGINAL_PROXY_ENV: OnceLock<HashMap<String, Option<String>>> = OnceLock::new();
    ORIGINAL_PROXY_ENV.get_or_init(|| {
        PROXY_ENV_KEYS
            .iter()
            .map(|key| ((*key).to_string(), std::env::var(key).ok()))
            .collect()
    })
}

fn set_current_proxy_settings(settings: &NetworkProxySettings) {
    if let Ok(mut guard) = current_proxy_settings_cell().lock() {
        *guard = settings.clone();
    }
}

pub(crate) fn current_proxy_settings() -> NetworkProxySettings {
    current_proxy_settings_cell()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn normalize_custom_proxy_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    let parsed = reqwest::Url::parse(&candidate).ok()?;
    let host = parsed.host_str()?;
    if host.trim().is_empty() {
        return None;
    }

    Some(parsed.to_string().trim_end_matches('/').to_string())
}

fn effective_proxy_mode(settings: &NetworkProxySettings) -> EffectiveProxyMode {
    if let Some(url) = normalize_custom_proxy_url(&settings.custom_proxy_url) {
        return EffectiveProxyMode::Custom(url);
    }
    if settings.use_system_proxy {
        return EffectiveProxyMode::System;
    }
    EffectiveProxyMode::Disabled
}

fn desired_proxy_env(
    snapshot: &HashMap<String, Option<String>>,
    mode: &EffectiveProxyMode,
) -> HashMap<String, Option<String>> {
    let mut next = HashMap::with_capacity(PROXY_ENV_KEYS.len());

    match mode {
        EffectiveProxyMode::Custom(url) => {
            for key in PROXY_ENV_KEYS {
                next.insert(key.to_string(), Some(url.clone()));
            }
        }
        EffectiveProxyMode::System => {
            for key in PROXY_ENV_KEYS {
                next.insert(key.to_string(), snapshot.get(key).cloned().flatten());
            }
        }
        EffectiveProxyMode::Disabled => {
            for key in PROXY_ENV_KEYS {
                next.insert(key.to_string(), None);
            }
        }
    }

    next
}

fn apply_proxy_env(mode: &EffectiveProxyMode) {
    let desired = desired_proxy_env(original_proxy_env_snapshot(), mode);
    for key in PROXY_ENV_KEYS {
        match desired.get(key).cloned().flatten() {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

pub(crate) fn apply_runtime_proxy_settings(settings: &NetworkProxySettings) {
    let normalized = normalize_settings(settings.clone());
    if !normalized.custom_proxy_url.is_empty()
        && normalize_custom_proxy_url(&normalized.custom_proxy_url).is_none()
    {
        log::warn!("忽略无效自定义代理地址: {}", normalized.custom_proxy_url);
    }
    let mode = effective_proxy_mode(&normalized);
    set_current_proxy_settings(&normalized);
    apply_proxy_env(&mode);
}

fn load_proxy_settings_from_conn(conn: &Connection) -> Result<NetworkProxySettings, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![NETWORK_PROXY_SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取代理设置失败: {error}"))?;
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(NetworkProxySettings::default());
    };
    serde_json::from_str::<NetworkProxySettings>(&raw)
        .map(normalize_settings)
        .map_err(|error| format!("解析代理设置失败: {error}"))
}

fn save_proxy_settings_to_conn(
    conn: &Connection,
    settings: &NetworkProxySettings,
) -> Result<(), String> {
    let json =
        serde_json::to_string(settings).map_err(|error| format!("序列化代理设置失败: {error}"))?;
    let now = crate::chrono_like_timestamp();
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![NETWORK_PROXY_SETTINGS_KEY, json, now],
    )
    .map_err(|error| format!("保存代理设置失败: {error}"))?;
    Ok(())
}

pub fn load_proxy_settings(app: &AppHandle) -> Result<NetworkProxySettings, String> {
    let connection = crate::open_history_db(app)?;
    load_proxy_settings_from_conn(&connection)
}

pub fn save_proxy_settings(
    app: &AppHandle,
    settings: &NetworkProxySettings,
) -> Result<NetworkProxySettings, String> {
    let normalized = normalize_settings(settings.clone());
    let connection = crate::open_history_db(app)?;
    save_proxy_settings_to_conn(&connection, &normalized)?;
    apply_runtime_proxy_settings(&normalized);
    Ok(normalized)
}

pub fn apply_saved_proxy_settings(app: &AppHandle) -> Result<NetworkProxySettings, String> {
    let settings = load_proxy_settings(app)?;
    apply_runtime_proxy_settings(&settings);
    Ok(settings)
}

fn build_http_client_for_settings(settings: &NetworkProxySettings) -> reqwest::Client {
    let mut builder = reqwest::Client::builder();

    match effective_proxy_mode(settings) {
        EffectiveProxyMode::Disabled => {
            builder = builder.no_proxy();
        }
        EffectiveProxyMode::System => {}
        EffectiveProxyMode::Custom(proxy_url) => match reqwest::Proxy::all(&proxy_url) {
            Ok(proxy) => {
                builder = builder.proxy(proxy);
            }
            Err(error) => {
                log::warn!("创建自定义代理失败 {proxy_url}: {error}");
                if !settings.use_system_proxy {
                    builder = builder.no_proxy();
                }
            }
        },
    }

    builder.build().unwrap_or_else(|error| {
        log::warn!("创建 HTTP client 失败，回退默认 client: {error}");
        reqwest::Client::new()
    })
}

pub(crate) fn build_http_client() -> reqwest::Client {
    build_http_client_for_settings(&current_proxy_settings())
}

fn build_blocking_http_client_for_settings(
    settings: &NetworkProxySettings,
) -> reqwest::blocking::Client {
    let mut builder = reqwest::blocking::Client::builder();

    match effective_proxy_mode(settings) {
        EffectiveProxyMode::Disabled => {
            builder = builder.no_proxy();
        }
        EffectiveProxyMode::System => {}
        EffectiveProxyMode::Custom(proxy_url) => match reqwest::Proxy::all(&proxy_url) {
            Ok(proxy) => {
                builder = builder.proxy(proxy);
            }
            Err(error) => {
                log::warn!("创建自定义阻塞代理失败 {proxy_url}: {error}");
                if !settings.use_system_proxy {
                    builder = builder.no_proxy();
                }
            }
        },
    }

    builder.build().unwrap_or_else(|error| {
        log::warn!("创建阻塞 HTTP client 失败，回退默认 client: {error}");
        reqwest::blocking::Client::new()
    })
}

pub(crate) fn build_blocking_http_client() -> reqwest::blocking::Client {
    build_blocking_http_client_for_settings(&current_proxy_settings())
}

pub async fn test_proxy_connection(settings: &NetworkProxySettings) -> Result<String, String> {
    let normalized = normalize_settings(settings.clone());
    if !normalized.custom_proxy_url.is_empty()
        && normalize_custom_proxy_url(&normalized.custom_proxy_url).is_none()
    {
        return Err(
            "自定义代理地址格式无效，支持 127.0.0.1:7890、http://host:port 或 socks5://host:port"
                .to_string(),
        );
    }

    let client = build_http_client_for_settings(&normalized);
    let response = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .map_err(|error| format!("连接测试失败: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("连接测试失败，目标返回状态码 {status}"));
    }

    let payload = response
        .json::<IpifyResponse>()
        .await
        .map_err(|error| format!("读取代理测试响应失败: {error}"))?;

    let message = match effective_proxy_mode(&normalized) {
        EffectiveProxyMode::Custom(url) => {
            format!("已通过自定义代理 {url} 访问外网，出口 IP：{}", payload.ip)
        }
        EffectiveProxyMode::System => {
            format!("已通过系统代理/系统网络访问外网，出口 IP：{}", payload.ip)
        }
        EffectiveProxyMode::Disabled => {
            format!("当前未启用代理，已直连访问外网，出口 IP：{}", payload.ip)
        }
    };

    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_host_port_without_scheme() {
        assert_eq!(
            normalize_custom_proxy_url("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            normalize_custom_proxy_url(" socks5://127.0.0.1:7890/ ").as_deref(),
            Some("socks5://127.0.0.1:7890")
        );
    }

    #[test]
    fn custom_proxy_has_priority_over_system_proxy() {
        let mode = effective_proxy_mode(&NetworkProxySettings {
            use_system_proxy: true,
            custom_proxy_url: "127.0.0.1:7890".to_string(),
        });
        assert_eq!(
            mode,
            EffectiveProxyMode::Custom("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn desired_env_restores_original_values_for_system_mode() {
        let snapshot = HashMap::from([
            (
                "http_proxy".to_string(),
                Some("http://from-shell:7890".to_string()),
            ),
            ("https_proxy".to_string(), None),
            ("all_proxy".to_string(), None),
            (
                "HTTP_PROXY".to_string(),
                Some("http://FROM-SHELL:7890".to_string()),
            ),
            ("HTTPS_PROXY".to_string(), None),
            ("ALL_PROXY".to_string(), None),
        ]);

        let desired = desired_proxy_env(&snapshot, &EffectiveProxyMode::System);

        assert_eq!(
            desired.get("http_proxy").cloned().flatten().as_deref(),
            Some("http://from-shell:7890")
        );
        assert_eq!(
            desired.get("HTTP_PROXY").cloned().flatten().as_deref(),
            Some("http://FROM-SHELL:7890")
        );
        assert_eq!(desired.get("https_proxy").cloned().flatten(), None);
    }

    #[test]
    fn persists_proxy_settings_round_trip() {
        let conn = crate::storage::db::open_in_memory().expect("open in-memory db");
        let settings = NetworkProxySettings {
            use_system_proxy: true,
            custom_proxy_url: "127.0.0.1:7890".to_string(),
        };

        save_proxy_settings_to_conn(&conn, &settings).expect("save settings");
        let loaded = load_proxy_settings_from_conn(&conn).expect("load settings");

        assert_eq!(loaded, settings);
    }
}
