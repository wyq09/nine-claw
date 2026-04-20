use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PiTokenUsagePayload {
    #[serde(alias = "inputTokens", alias = "input", default)]
    pub(crate) input_tokens: Option<u64>,
    #[serde(alias = "outputTokens", alias = "output", default)]
    pub(crate) output_tokens: Option<u64>,
    #[serde(
        alias = "cacheReadTokens",
        alias = "cacheRead",
        alias = "cache_read_tokens",
        default
    )]
    pub(crate) cache_read_tokens: Option<u64>,
    #[serde(
        alias = "cacheWriteTokens",
        alias = "cacheWrite",
        alias = "cache_write_tokens",
        default
    )]
    pub(crate) cache_write_tokens: Option<u64>,
    #[serde(alias = "totalTokens", alias = "total_tokens", default)]
    pub(crate) total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PiUsageMetadataPayload {
    pub(crate) api: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    #[serde(alias = "responseId", alias = "response_id", default)]
    pub(crate) response_id: Option<String>,
    pub(crate) timestamp: Option<i64>,
}

pub(crate) fn json_i64(value: Option<&serde_json::Value>) -> Option<i64> {
    value.and_then(|item| {
        item.as_i64()
            .or_else(|| item.as_u64().and_then(|number| i64::try_from(number).ok()))
    })
}

pub(crate) fn json_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|item| {
        item.as_u64()
            .or_else(|| item.as_i64().and_then(|number| u64::try_from(number).ok()))
    })
}

pub(crate) fn extract_usage_payload(
    value: Option<&serde_json::Value>,
) -> Option<PiTokenUsagePayload> {
    let usage = value?;
    let input_tokens = extract_json_u64(
        usage
            .get("input")
            .or_else(|| usage.get("input_tokens"))
            .or_else(|| usage.get("inputTokens")),
    );
    let output_tokens = extract_json_u64(
        usage
            .get("output")
            .or_else(|| usage.get("output_tokens"))
            .or_else(|| usage.get("outputTokens")),
    );
    let cache_read_tokens = extract_json_u64(
        usage
            .get("cacheRead")
            .or_else(|| usage.get("cache_read_tokens"))
            .or_else(|| usage.get("cacheReadTokens")),
    );
    let cache_write_tokens = extract_json_u64(
        usage
            .get("cacheWrite")
            .or_else(|| usage.get("cache_write_tokens"))
            .or_else(|| usage.get("cacheWriteTokens")),
    );
    let total_tokens = extract_json_u64(
        usage
            .get("totalTokens")
            .or_else(|| usage.get("total_tokens")),
    )
    .or_else(|| {
        Some(
            input_tokens.unwrap_or(0)
                + output_tokens.unwrap_or(0)
                + cache_read_tokens.unwrap_or(0)
                + cache_write_tokens.unwrap_or(0),
        )
    });

    if input_tokens.is_none()
        && output_tokens.is_none()
        && cache_read_tokens.is_none()
        && cache_write_tokens.is_none()
        && total_tokens.unwrap_or(0) == 0
    {
        return None;
    }

    Some(PiTokenUsagePayload {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        total_tokens,
    })
}

pub(crate) fn usage_row_total_tokens(payload: &PiTokenUsagePayload) -> u64 {
    payload.total_tokens.unwrap_or_else(|| {
        payload.input_tokens.unwrap_or(0)
            + payload.output_tokens.unwrap_or(0)
            + payload.cache_read_tokens.unwrap_or(0)
            + payload.cache_write_tokens.unwrap_or(0)
    })
}

fn merge_pi_token_usage_payloads(
    left: &PiTokenUsagePayload,
    right: &PiTokenUsagePayload,
) -> PiTokenUsagePayload {
    let input = left.input_tokens.unwrap_or(0) + right.input_tokens.unwrap_or(0);
    let output = left.output_tokens.unwrap_or(0) + right.output_tokens.unwrap_or(0);
    let cache_read = left.cache_read_tokens.unwrap_or(0) + right.cache_read_tokens.unwrap_or(0);
    let cache_write = left.cache_write_tokens.unwrap_or(0) + right.cache_write_tokens.unwrap_or(0);
    let total = usage_row_total_tokens(left) + usage_row_total_tokens(right);
    PiTokenUsagePayload {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cache_read_tokens: Some(cache_read),
        cache_write_tokens: Some(cache_write),
        total_tokens: Some(total),
    }
}

/// 将多轮 assistant 调用的用量相加（一次用户任务内可能有多条带 `usage` 的 assistant 消息）。
pub(crate) fn accumulate_pi_token_usage(
    into: &mut Option<PiTokenUsagePayload>,
    step: Option<PiTokenUsagePayload>,
) {
    let Some(step) = step else {
        return;
    };
    *into = Some(match into.take() {
        None => step,
        Some(prev) => merge_pi_token_usage_payloads(&prev, &step),
    });
}

/// 从 `agent_end` 的 `messages` 数组汇总所有 assistant 的 `usage`，并取最后一条 assistant 的元数据。
pub(crate) fn aggregate_usage_from_agent_messages(
    messages: &[serde_json::Value],
) -> (Option<PiTokenUsagePayload>, Option<PiUsageMetadataPayload>) {
    let mut total: Option<PiTokenUsagePayload> = None;
    let mut last_meta: Option<PiUsageMetadataPayload> = None;
    for message in messages {
        if message.get("role").and_then(|item| item.as_str()) != Some("assistant") {
            continue;
        }
        let step = extract_usage_payload(message.get("usage"));
        accumulate_pi_token_usage(&mut total, step);
        if let Some(meta) = extract_usage_metadata_payload(Some(message)) {
            last_meta = Some(meta);
        }
    }
    (total, last_meta)
}

pub(crate) fn extract_usage_metadata_payload(
    value: Option<&serde_json::Value>,
) -> Option<PiUsageMetadataPayload> {
    let payload = value?;
    let api = json_string(payload.get("api"));
    let provider = json_string(payload.get("provider"));
    let model = json_string(payload.get("model"));
    let response_id = json_string(
        payload
            .get("responseId")
            .or_else(|| payload.get("response_id")),
    );
    let timestamp = json_i64(payload.get("timestamp"));

    if api.is_none()
        && provider.is_none()
        && model.is_none()
        && response_id.is_none()
        && timestamp.is_none()
    {
        return None;
    }

    Some(PiUsageMetadataPayload {
        api,
        provider,
        model,
        response_id,
        timestamp,
    })
}
