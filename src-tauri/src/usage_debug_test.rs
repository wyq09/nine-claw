// Standalone test binary — run with: cargo run -p app --bin usage_debug

use serde_json::json;

fn extract_json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|item| {
        item.as_u64()
            .or_else(|| item.as_i64().and_then(|number| u64::try_from(number).ok()))
    })
}

fn extract_usage_payload(value: Option<&serde_json::Value>) -> Option<String> {
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

    Some(format!(
        "input={}, output={}, cache_read={}, cache_write={}, total={}",
        input_tokens.map(|v| v.to_string()).unwrap_or_default(),
        output_tokens.map(|v| v.to_string()).unwrap_or_default(),
        cache_read_tokens.map(|v| v.to_string()).unwrap_or_default(),
        cache_write_tokens.map(|v| v.to_string()).unwrap_or_default(),
        total_tokens.unwrap_or(0),
    ))
}

fn main() {
    // Test 1: PI message_end payload (actual data from logs)
    let message_end_payload = json!({
        "type": "message_end",
        "message": {
            "role": "assistant",
            "content": [{"type": "text", "text": "hello"}],
            "api": "nineclaw-anthropic-compat",
            "provider": "nineclaw-compat-2474aa3e67ab",
            "model": "GLM-5-Turbo",
            "usage": {
                "input": 269,
                "output": 27,
                "cacheRead": 8971,
                "cacheWrite": 0,
                "totalTokens": 9267,
                "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
            },
            "stopReason": "stop",
            "timestamp": 1775815383630i64,
            "responseId": "msg_xxx"
        }
    });

    // Test 2: message field extraction
    let message = message_end_payload.get("message");
    let usage_from_message = message.and_then(|m| m.get("usage"));
    println!("=== Test 1: message_end → message → usage ===");
    println!("message present: {}", message.is_some());
    println!("usage from message: {}", usage_from_message.map(|v| v.to_string()).unwrap_or_default());
    println!("extract result: {:?}", extract_usage_payload(usage_from_message));

    // Test 3: agent_end payload (actual data from logs)
    let agent_end_payload = json!({
        "type": "agent_end",
        "messages": [
            {
                "role": "user",
                "content": [{"type": "text", "text": "hi"}],
                "timestamp": 1775806959828i64
            },
            {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hi!"}],
                "api": "nineclaw-anthropic-compat",
                "provider": "nineclaw-compat-2474aa3e67ab",
                "model": "GLM-5-Turbo",
                "usage": {
                    "input": 5,
                    "output": 142,
                    "cacheRead": 9732,
                    "cacheWrite": 0,
                    "totalTokens": 9879,
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
                },
                "stopReason": "stop",
                "timestamp": 1775806959830i64,
                "responseId": "msg_20260410154240353d6e0e4912428d"
            }
        ]
    });

    // Test 4: messages array extraction (code path in agent_end handler)
    let messages = agent_end_payload.get("messages").and_then(|m| m.as_array());
    let last_assistant = messages.and_then(|msgs| {
        msgs.iter().rev().find(|msg| {
            msg.get("role").and_then(|r| r.as_str()) == Some("assistant")
        })
    });
    let usage_from_agent_end = last_assistant.and_then(|a| a.get("usage"));
    println!("\n=== Test 3: agent_end → messages → last_assistant → usage ===");
    println!("messages present: {}", messages.is_some());
    println!("last_assistant present: {}", last_assistant.is_some());
    println!("usage from last_assistant: {}", usage_from_agent_end.map(|v| v.to_string()).unwrap_or_default());
    println!("extract result: {:?}", extract_usage_payload(usage_from_agent_end));

    // Test 5: What the code actually does — line_type == "message_end", value.get("message")
    println!("\n=== Test 5: Code path simulation for message_end ===");
    let line_type = message_end_payload.get("type").and_then(|t| t.as_str()).unwrap_or_default();
    println!("line_type = '{}'", line_type);
    println!("matches message_start|message_end|turn_end: {}",
        matches!(line_type, "message_start" | "message_end" | "turn_end"));
    let msg = message_end_payload.get("message");
    let is_assistant = msg.and_then(|m| m.get("role")).and_then(|r| r.as_str()) == Some("assistant");
    println!("is_assistant: {}", is_assistant);
    let raw_usage = msg.and_then(|m| m.get("usage"));
    println!("raw_usage: {}", raw_usage.map(|v| v.to_string()).unwrap_or_default());
    println!("extract_usage: {:?}", extract_usage_payload(raw_usage));

    // Test 6: message_update.done path
    println!("\n=== Test 6: Code path simulation for message_update ===");
    let message_update_payload = json!({
        "type": "message_update",
        "assistantMessageEvent": {
            "type": "done",
            "message": {
                "role": "assistant",
                "usage": {
                    "input": 269,
                    "output": 27,
                    "cacheRead": 8971,
                    "cacheWrite": 0,
                    "totalTokens": 9267
                }
            }
        }
    });
    let assistant_event = message_update_payload.get("assistantMessageEvent");
    let delta_type = assistant_event.and_then(|e| e.get("type")).and_then(|t| t.as_str()).unwrap_or_default();
    println!("delta_type = '{}'", delta_type);
    if delta_type == "done" {
        let usage_via_assistant_event = assistant_event
            .and_then(|e| e.get("message"))
            .and_then(|m| m.get("usage"));
        println!("usage via assistantMessageEvent.message.usage: {}", usage_via_assistant_event.map(|v| v.to_string()).unwrap_or_default());
        println!("extract_usage: {:?}", extract_usage_payload(usage_via_assistant_event));
    }

    // Test 7: response command path (desktop_pi_rpc)
    println!("\n=== Test 7: response command=done path ===");
    let response_done_payload = json!({
        "type": "response",
        "command": "done",
        "success": true,
        "message": {
            "role": "assistant",
            "usage": {
                "input": 269,
                "output": 27,
                "cacheRead": 8971,
                "cacheWrite": 0,
                "totalTokens": 9267
            }
        }
    });
    let cmd = response_done_payload.get("command").and_then(|c| c.as_str()).unwrap_or_default();
    let response_msg = response_done_payload.get("message");
    println!("command = '{}'", cmd);
    println!("response_msg present: {}", response_msg.is_some());
    let is_resp_assistant = response_msg.and_then(|m| m.get("role")).and_then(|r| r.as_str()) == Some("assistant");
    println!("is_assistant: {}", is_resp_assistant);
    let raw = response_msg.and_then(|m| m.get("usage"));
    println!("raw_usage: {}", raw.map(|v| v.to_string()).unwrap_or_default());
    println!("extract_usage: {:?}", extract_usage_payload(raw));

    // Test 8: Check the "response" block's last_assistant logic
    println!("\n=== Test 8: response block last_assistant path ===");
    let response_with_messages = json!({
        "type": "response",
        "command": "done",
        "success": true,
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "hi"}]},
            {
                "role": "assistant",
                "content": [{"type": "text", "text": "hello"}],
                "usage": {
                    "input": 269,
                    "output": 27,
                    "cacheRead": 8971,
                    "cacheWrite": 0,
                    "totalTokens": 9267
                }
            }
        ]
    });
    let msgs_array = response_with_messages.get("messages").and_then(|m| m.as_array());
    let last_ass = msgs_array.and_then(|msgs| {
        msgs.iter().rev().find(|msg| {
            msg.get("role").and_then(|r| r.as_str()) == Some("assistant")
        })
    });
    let usage_last = last_ass.and_then(|a| a.get("usage"));
    println!("via messages[].find(last_assistant): {}", usage_last.map(|v| v.to_string()).unwrap_or_default());
    println!("extract_usage: {:?}", extract_usage_payload(usage_last));
}
