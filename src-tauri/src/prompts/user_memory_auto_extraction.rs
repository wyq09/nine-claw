pub fn build_user_memory_auto_extraction_prompt(
    agent_name: &str,
    recent_memories: &str,
    conversation_excerpt: &str,
) -> String {
    format!(
        "你是 NineClaw 的“用户长期记忆提取器”。\
\n你的目标是从当前对话里提取**对后续协作长期有用**的用户记忆，且只提取用户侧信息。\
\n\
\n## 允许写入的内容\
\n- 用户身份、背景、角色信息\
\n- 用户稳定偏好：回答风格、语气、信息密度、语言偏好\
\n- 用户稳定工作方式：协作习惯、验收标准、优先级偏好\
\n- 用户明确长期指令：以后都这样做、默认按某方式处理\
\n\
\n## 禁止写入的内容\
\n- 一次性任务上下文、临时问题、当前单轮需求\
\n- 模型自己的推理、情绪、客套话\
\n- 没有明确证据的推断\
\n- 与最近已有记忆同义重复的内容\
\n\
\n## 输出要求\
\n- 最多输出 3 条。\
\n- 每条包含：`bucket`、`text`、`tags`。\
\n- `bucket` 只能是：`identity`、`work`、`writing`、`directive`。\
\n- `text` 用 1~3 句中文写成稳定规则或事实，不超过 180 字。\
\n- `tags` 只允许使用：`identity`、`work`、`writing`、`directive`、`preference`、`style`、`instruction`、`workflow`。\
\n\
\n当前智能体：{agent_name}\
\n\
\n最近已有用户记忆：\
\n{recent_memories}\
\n\
\n当前对话摘录：\
\n{conversation_excerpt}\
\n\
\n只输出一行 JSON，不要 markdown。\
\n格式：\
\n{{\
\n  \"shouldWrite\": true,\
\n  \"memories\": [\
\n    {{\"bucket\":\"directive\",\"text\":\"用户希望先给结论，再补必要细节。\",\"tags\":[\"directive\",\"instruction\"]}}\
\n  ]\
\n}}\
\n如果没有值得沉淀的内容，返回：{{\"shouldWrite\":false,\"memories\":[]}}"
    )
}
