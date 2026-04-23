pub fn build_workspace_memory_extraction_prompt(
    workspace_name: &str,
    supervisor_name: &str,
    recent_memories: &str,
    conversation_excerpt: &str,
) -> String {
    format!(
        "你是 NineClaw 团队空间的“共享记忆提取与分类器”。\
\n你的任务是：根据下面的团队对话片段，只提取**值得长期沉淀**到团队共享记忆里的内容。\
\n\
\n提取标准：\
\n- 只保留对后续协作真正有复用价值的信息：明确决策、长期约束、关键事实、成员分工、稳定偏好、重要资源路径、后续计划、持续风险、团队工作流规则。\
\n- 忽略寒暄、一次性客套、过程性废话、模型自述、即时情绪、纯粹的思考过程、临时措辞润色。\
\n- 不要编造对话中没有出现的事实。\
\n- 如果没有可沉淀内容，返回空数组。\
\n- 尽量避免和“最近已有共享记忆”重复；同义改写但信息未新增，也视为重复。\
\n\
\n分类要求：\
\n- 每条记忆都要给出 `tags` 数组。\
\n- `tags` 只允许从以下集合中选择 1~4 个：`decision`, `constraint`, `fact`, `preference`, `resource`, `plan`, `risk`, `workflow`, `people`。\
\n- `title`：4~24 个字，短标题。\
\n- `content`：1~4 句中文，写成便于团队复用的摘要，不超过 220 个字。\
\n- 最多输出 3 条记忆。\
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
\n格式：{{\"memories\":[{{\"title\":\"...\",\"content\":\"...\",\"tags\":[\"decision\"]}}]}}"
    )
}
