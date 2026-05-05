use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

#[derive(Debug)]
enum PendingWidgetResolution {
    Submitted { answers: Value },
    Cancelled,
}

#[derive(Debug)]
struct PendingWidgetRequest {
    session_id: String,
    widget: Value,
    responder: oneshot::Sender<PendingWidgetResolution>,
}

fn pending_widget_requests() -> &'static Mutex<HashMap<String, PendingWidgetRequest>> {
    static STORE: OnceLock<Mutex<HashMap<String, PendingWidgetRequest>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn widget_id_of(widget: &Value) -> Option<&str> {
    widget
        .get("widgetId")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

fn widget_with_status(widget: &Value, status: &str) -> Value {
    let mut next = widget.clone();
    if let Some(map) = next.as_object_mut() {
        map.insert("status".to_string(), Value::String(status.to_string()));
    }
    next
}

fn emit_widget_stream_event(
    app: &AppHandle,
    event: &str,
    session_id: &str,
    widget: &Value,
) -> Result<(), String> {
    app.emit(
        "pi://stream",
        json!({
            "event": event,
            "session_id": session_id,
            "widget": widget,
        }),
    )
    .map_err(|error| format!("发送 widget 流事件失败: {error}"))
}

pub async fn create_pending_widget_request(
    app: &AppHandle,
    session_id: &str,
    widget: Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    let widget_id = widget_id_of(&widget)
        .ok_or_else(|| "widget 缺少 widgetId".to_string())?
        .to_string();
    let (tx, rx) = oneshot::channel();

    {
        let mut guard = pending_widget_requests()
            .lock()
            .map_err(|error| format!("锁定待处理 widget 请求失败: {error}"))?;
        guard.insert(
            widget_id.clone(),
            PendingWidgetRequest {
                session_id: session_id.to_string(),
                widget: widget.clone(),
                responder: tx,
            },
        );
    }

    emit_widget_stream_event(app, "widget_request", session_id, &widget)?;

    match timeout(Duration::from_millis(timeout_ms), rx).await {
        Ok(Ok(PendingWidgetResolution::Submitted { answers })) => Ok(json!({
            "ok": true,
            "widgetId": widget_id,
            "answers": answers,
        })),
        Ok(Ok(PendingWidgetResolution::Cancelled)) | Ok(Err(_)) => Ok(json!({
            "ok": false,
            "widgetId": widget_id,
            "reason": "cancelled",
        })),
        Err(_) => {
            let expired_widget = {
                let mut guard = pending_widget_requests()
                    .lock()
                    .map_err(|error| format!("锁定待处理 widget 请求失败: {error}"))?;
                guard.remove(&widget_id).map(|pending| {
                    (
                        pending.session_id,
                        widget_with_status(&pending.widget, "expired"),
                    )
                })
            };
            if let Some((expired_session_id, expired_widget)) = expired_widget {
                emit_widget_stream_event(
                    app,
                    "widget_resolved",
                    &expired_session_id,
                    &expired_widget,
                )?;
            }
            Ok(json!({
                "ok": false,
                "widgetId": widget_id,
                "reason": "expired",
            }))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSubmitPayload {
    pub widget_id: String,
    pub kind: String,
    pub answers: Value,
}

#[tauri::command]
pub async fn widget_submit_response(
    app: AppHandle,
    payload: WidgetSubmitPayload,
) -> Result<(), String> {
    if payload.kind.trim() != "ask_user" {
        return Err(format!("暂不支持的 widget kind: {}", payload.kind));
    }
    let pending = {
        let mut guard = pending_widget_requests()
            .lock()
            .map_err(|error| format!("锁定待处理 widget 请求失败: {error}"))?;
        guard.remove(payload.widget_id.trim())
    };
    let Some(pending) = pending else {
        return Err("未找到待处理 widget，可能已提交、取消或过期".to_string());
    };

    let resolved_widget = widget_with_status(&pending.widget, "submitted");
    emit_widget_stream_event(
        &app,
        "widget_resolved",
        &pending.session_id,
        &resolved_widget,
    )?;
    pending
        .responder
        .send(PendingWidgetResolution::Submitted {
            answers: payload.answers,
        })
        .map_err(|_| "widget 等待通道已关闭".to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetCancelPayload {
    pub widget_id: String,
    pub kind: String,
}

#[tauri::command]
pub async fn widget_cancel_response(
    app: AppHandle,
    payload: WidgetCancelPayload,
) -> Result<(), String> {
    if payload.kind.trim() != "ask_user" {
        return Err(format!("暂不支持的 widget kind: {}", payload.kind));
    }
    let pending = {
        let mut guard = pending_widget_requests()
            .lock()
            .map_err(|error| format!("锁定待处理 widget 请求失败: {error}"))?;
        guard.remove(payload.widget_id.trim())
    };
    let Some(pending) = pending else {
        return Err("未找到待处理 widget，可能已提交、取消或过期".to_string());
    };

    let resolved_widget = widget_with_status(&pending.widget, "cancelled");
    emit_widget_stream_event(
        &app,
        "widget_resolved",
        &pending.session_id,
        &resolved_widget,
    )?;
    pending
        .responder
        .send(PendingWidgetResolution::Cancelled)
        .map_err(|_| "widget 等待通道已关闭".to_string())
}

pub fn build_ask_user_widget(
    title: String,
    description: Option<String>,
    submit_label: Option<String>,
    cancel_label: Option<String>,
    allow_skip: bool,
    questions: Value,
) -> Result<Value, String> {
    let questions = questions
        .as_array()
        .cloned()
        .ok_or_else(|| "ask_user questions 必须是数组".to_string())?;
    if questions.is_empty() {
        return Err("ask_user questions 不能为空".to_string());
    }

    Ok(json!({
        "kind": "ask_user",
        "widgetId": format!("ask_user_{}", Uuid::new_v4().simple()),
        "version": 1,
        "title": title,
        "description": description,
        "submitLabel": submit_label,
        "cancelLabel": cancel_label,
        "allowSkip": allow_skip,
        "status": "pending",
        "questions": questions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_ask_user_widget_creates_pending_widget() {
        let widget = build_ask_user_widget(
            "继续前确认一下".to_string(),
            Some("请选择一种方式".to_string()),
            Some("继续".to_string()),
            Some("跳过".to_string()),
            true,
            json!([{ "id": "format", "type": "single_select", "label": "格式", "options": [{ "id": "doc", "label": "Word" }, { "id": "other", "label": "其他" }] }]),
        )
        .expect("build widget");

        assert_eq!(widget.get("kind").and_then(Value::as_str), Some("ask_user"));
        assert_eq!(
            widget.get("status").and_then(Value::as_str),
            Some("pending")
        );
        assert!(widget_id_of(&widget).is_some());
    }

    #[test]
    fn widget_with_status_updates_status_field() {
        let widget = json!({
            "widgetId": "ask_1",
            "status": "pending",
        });

        let submitted = widget_with_status(&widget, "submitted");
        assert_eq!(
            submitted.get("status").and_then(Value::as_str),
            Some("submitted")
        );
        assert_eq!(
            submitted.get("widgetId").and_then(Value::as_str),
            Some("ask_1")
        );
    }
}
