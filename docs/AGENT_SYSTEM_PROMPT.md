# 智能体系统提示词 — 配置位置说明

## 你主要改哪里（唯一主入口）

**在应用内「智能体」编辑里填写「系统指令」**（对应数据库 `agents.system_prompt`）。这是**你自定义的执行约束、风格、禁区**的主配置面，会完整进入 `agent_system_prompt` 段（有合理总长度上限，见下方）。

名字、简介、能力策略、已选技能等会由产品自动拼成前面的摘要段；**长文、细则请写在「系统指令」里**，不要只依赖简介栏。

## 系统消息里还有啥（别和「系统指令」混为一谈）

同一轮对话里，runtime 会**顺序追加**多段 `append-system-prompt`（在 LLM 追踪里可见各段 label）：

1. `agent_system_prompt` — 上面主入口 + 自动摘要 +（若开启）工作区相关拼接
2. `memory_isolation` — 记忆隔离规则
3. `workspace_team` — 仅在团队工作区场景
4. `runtime_skills` — 当前轮挂载技能说明
5. 桌面/媒体等其它产品注入段

**工作区 Markdown**（`~/.nineclaw/workspace/agents/<id>/` 下的 `ROLE.md`、`IDENTITY.md`、记忆文件等）用于**持续身份、记忆与工具可读内容**；其中一部分会经摘要进入上下文，**不等同于**你在 UI 里写的那一条「系统指令」全文。长叙事与分工建议维护在 ROLE / 各 md 里，**与 UI「系统指令」配合**：指令写「必须怎么做事」，工作区写「是谁、记了什么」。

## 长度上限

- 用户「系统指令」正文硬上限约 **32 000 字符**（防误粘贴超大文本）；超过会截断并在末尾附一句提示。

## 与仓库内其它 `AGENTS.md` 的关系

- 仓库根目录的 `AGENTS.md` 是给**人类开发者 / 外部编程 Agent** 的协作说明，**不**会注入 NineClaw 里聊天智能体的 system prompt。

更多目录与文件职责见 [AGENT_WORKSPACE_ARCHITECTURE.md](./AGENT_WORKSPACE_ARCHITECTURE.md)。
