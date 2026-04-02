pub(crate) mod config;

use anyhow::Result;
use rand::Rng;

/// Look up the MCP URL for the given `category` (matched against `biz_type`).
pub async fn get_mcp_url(category: &str) -> Result<String> {
    let resp = config::get_mcp_config().await?;

    let target = resp
        .list
        .iter()
        .find(|item| item.biz_type == category)
        .ok_or_else(|| anyhow::anyhow!("当前企业暂不支持 {category} 命令"))?;

    Ok(target.url.clone())
}

/// Generate a request ID in the format: `{prefix}_{timestamp_ms}_{random_hex}`.
pub fn gen_req_id(prefix: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let random = generate_random_hex(8);
    format!("{prefix}_{timestamp}_{random}")
}

/// Generate a random hex string of the specified character length.
fn generate_random_hex(length: usize) -> String {
    let byte_len = (length + 1) / 2;
    let bytes: Vec<u8> = (0..byte_len).map(|_| rand::rng().random::<u8>()).collect();
    let hex = hex::encode(bytes);
    hex[..length].to_string()
}
