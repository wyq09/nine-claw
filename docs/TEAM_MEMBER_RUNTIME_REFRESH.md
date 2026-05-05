# Team Member Runtime Refresh

## Problem

团队会话里的当前发言 agent 之前直接复用前端传来的会话快照。这样会有两个副作用：

- 对话进行中，如果团队新增成员，后端虽然会同步主智能体的委派白名单，但当前会话下一轮仍可能带着旧的 agent 配置继续跑。
- 如果某个 agent 的专属系统提示词里手写了“你的团队成员”，它可能和运行时动态注入的真实团队名单冲突，模型会被旧名单误导。

## What Changed

### 1. Runtime agent config refresh

新增 `src-tauri/src/runtime_agent_config.rs`，在每次 `stream_pi_prompt` 进入时：

- 只要请求里带了 `agentId`
- 就回库读取最新的 `ConversationAgentConfig`
- 若数据库里仍存在该 agent，则以最新配置覆盖会话快照
- 若 agent 已不存在，才回退到原快照

这让以下变更可以在当前会话的下一轮立刻生效：

- 主智能体委派白名单同步
- system prompt 更新
- skill / tool / collaboration 配置更新

### 2. Dynamic team member list wins

`src-tauri/src/team_workspace.rs` 现在在团队会话前言里显式声明：

- 任何其他系统提示词、历史摘要、手写规则里的成员名单都只能视为旧注释
- `## 成员` 和 `## 当前可调用子智能体` 才是当前轮次唯一真源

这样即使某个旧 prompt 里残留了过时的成员名单，模型也会优先服从运行时动态团队表。

## Tests

新增测试覆盖：

- `refresh_prefers_live_config_when_available`
- `refresh_keeps_snapshot_when_live_config_missing`
- `team_member_truth_note_points_to_dynamic_sections`
