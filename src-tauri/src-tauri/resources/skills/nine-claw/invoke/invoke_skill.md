## invoke_skill 机制设计

### 1. 角色定义
- **主 agent**：负责接收用户输入、管理上下文、决定是否需要调用技能。
- **invoke_skill**：一个固定工具入口，接收 skill_name 和任务描述。
- **子 skill agent**（可选）：根据 skill 配置决定是否 fork 隔离执行。

### 2. 执行逻辑
**当主 agent 接收到用户请求后**：
1. 检查是否匹配任一 skill 的描述（如“总结文章”“生成 PPT”）。
2. 若匹配，调用 `invoke_skill(skill_name, task)`。
3. `invoke_skill` 内部判断 skill 的 `fork_agent` 标志：
   - 若 `false`：内联展开 skill 的 SKILL.md 内容，注入为 system 消息 + task 模拟用户输入，LLM 在主上下文继续运行。
   - 若 `true`：fork 一个独立子 agent，提供完整的 skill 系统提示与任务输入，子 agent 输出结果传回主 agent。

### 3. 优势
- **统一入口**：所有 skill 调用走同一个工具，schema 稳定，避免上下文爆炸。
- **灵活隔离**：复杂 heavy skill 走子 agent 保主历史简洁；轻量 skill 内联提升效率。
- **热加载**：skill 的 SKILL.md 文件是运行时读取，新增/更新无需重启。
- **上下文控制**：技能执行可指定 context_scope: [