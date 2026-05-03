pub fn build_workspace_memory_extraction_prompt(
    workspace_name: &str,
    supervisor_name: &str,
    recent_memories: &str,
    conversation_excerpt: &str,
) -> String {
    format!(
        "你是 NineClaw 团队空间的“共享记忆 gate + writer”。\
\n你的任务分两步，而且必须按顺序思考：\
\n1. 先做 gate：判断这段团队对话里，是否存在**值得长期沉淀**到团队共享记忆的内容。\
\n2. 再做 writer：只有 gate 通过，才输出真正要写入的记忆条目。\
\n\
\n## Gate 标准\
\n值得写入：明确决策、长期约束、关键事实、成员分工、稳定偏好、重要资源路径、后续计划、持续风险、团队工作流规则。\
\n不值得写入：寒暄、一次性客套、过程性废话、模型自述、即时情绪、纯思考过程、临时措辞润色、没有复用价值的单次互动。\
\n不要编造对话中没有出现的事实。\
\n尽量避免和“最近已有共享记忆”重复；同义改写但信息未新增，也视为重复。\
\n\
\n## Route 规则\
\n- `decision`: 明确拍板、政策或默认选择\
\n- `constraint`: 长期限制、必须遵守的边界、兼容性要求\
\n- `fact`: 稳定事实、背景事实、已确认状态\
\n- `preference`: 稳定偏好、输出习惯、合作偏好\
\n- `resource`: 重要路径、文档、仓库、环境入口、外部资源\
\n- `plan`: 后续计划、下一阶段方向、待推进方案\
\n- `risk`: 持续风险、已知坑、需要长期警惕的问题\
\n- `workflow`: 协作流程、交付习惯、操作规范\
\n- `people`: 成员职责、角色分工、关键联系人\
\n\
\n## Writer 要求\
\n- 最多输出 3 条记忆。\
\n- `title`：4~24 个字，短标题。\
\n- `content`：1~4 句中文，写成便于团队复用的摘要，不超过 220 个字。\
\n- 每条记忆都要给出 `route` 和 `tags`。\
\n- `route` 必须是上述 9 个 route 之一。\
\n- `tags` 只允许从以下集合中选择 1~4 个：`decision`, `constraint`, `fact`, `preference`, `resource`, `plan`, `risk`, `workflow`, `people`。\
\n\
\n## Scope 规则\
\n每条记忆需要判断所属层级：\
\n- `system`: 跨项目通用的用户偏好、个人特征、通用知识（如「用户偏好简洁回复」「用户叫张三」）\
\n- `workspace`: 项目级别的决策、事实、约束、计划（如「项目用 Rust + React」「向量化存储选择 sqlite-vec」）\
\n- `agent`: 特定智能体的执行经验、专属能力描述（如「XX 参数需要调 3 次才能成功」「这个 agent 擅长代码审查」）\
\n默认 `workspace`；除非有明确信号，不要轻易标记为 `system`。\
\n\
\n团队：{workspace_name}\
\n主智能体：{supervisor_name}\
\n\
\n最近已有共享记忆：\
\n{recent_memories}\
\n\
\n本轮提取参考对话：\
\n{conversation_excerpt}\
\n\
\n只输出一行合法 JSON，不要 markdown 代码块。\
\n使用统一的 `MemoryGate` schema：团队共享记忆关注 `decision / constraint / fact / preference / resource / plan / risk / workflow / people`；其余私有字段可省略或写 `false`。\
\n格式：\
\n{{\
\n  \"gate\": {{\
\n    \"should_write\": true,\
\n    \"decision\": false,\
\n    \"constraint\": false,\
\n    \"fact\": false,\
\n    \"preference\": false,\
\n    \"resource\": false,\
\n    \"plan\": false,\
\n    \"risk\": false,\
\n    \"workflow\": false,\
\n    \"people\": false\
\n  }},\
\n  \"memories\": [\
\n    {{\"route\":\"decision\",\"title\":\"...\",\"content\":\"...\",\"tags\":[\"decision\"],\"scope\":\"workspace\"}}\
\n  ]\
\n}}\
\n如果没有可沉淀内容，返回：{{\"gate\":{{\"should_write\":false}},\"memories\":[]}}"
    )
}
