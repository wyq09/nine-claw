pub fn stable_agent_dynamic_instruction_notice() -> &'static str {
    "专属执行要求：当前智能体配置了用户自定义执行要求；运行时会在每轮用户消息前方以 `agent_dynamic_instruction` 上下文块提供，必须遵守该上下文块。"
}

pub fn render_agent_dynamic_instruction_context(instruction: &str) -> String {
    format!(
        "最高优先级执行要求（MUST）：以下是用户为当前智能体配置的专属执行要求。除非违反平台安全或系统级限制，否则必须严格遵守；若与通用能力说明冲突，以本段为准。\n{}",
        instruction.trim()
    )
}
