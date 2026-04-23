//! Provider 流式结束时可能出现的非致命文案，避免误判为对用户可见的致命错误。

pub(crate) fn is_recoverable_provider_terminal_noise(message: &str) -> bool {
    let trimmed = message.trim();
    let lower = trimmed.to_lowercase();
    // stdout 已结束后 pi 子进程未及时退出，运行时会 SIGKILL；部分运行时会把助手结束原因写成 terminated。
    if lower == "terminated" || lower == "killed" || lower.ends_with(" terminated") {
        return true;
    }
    if lower.contains("unexpected_state") {
        return true;
    }
    if lower.contains("finish_reason")
        && (lower.contains("unexpected") || lower.contains("unknown"))
    {
        return true;
    }
    if lower.contains("provider") && lower.contains("unexpected") {
        return true;
    }
    if lower.contains("stream") && lower.contains("incomplete") {
        return true;
    }
    false
}
