# NineClaw

一个基于 **Tauri 2.0 + React + TypeScript** 的桌面测试项目，已接入 `pi`，前端采用 Codex 风格布局（侧边栏 + 聊天区 + 底部输入区）。

## 功能

- 前端输入 prompt
- 通过 Tauri `invoke` 调用 Rust command：`stream_pi_prompt`
- Rust 以 RPC 模式启动 `pi`：
  - `pi --mode rpc --session <temp-session-file>`
- 前端通过 Tauri 事件 `pi://stream` 实时接收 `text_delta`
- 支持“停止生成”（优先发送 RPC `abort` 指令）
- 支持会话保持（多轮对话上下文）
- 支持区分“用户手动中止”和“模型中断”
- 支持“重试上一条”
- 支持本地历史面板（可点选重放，数据保存在 localStorage）
- 支持“清空会话”按钮
- 支持自定义智能体管理
- 支持为每个智能体生成独立的 markdown 工作区与记忆骨架

## 前置要求

1. 安装 Rust 与 Tauri 依赖环境
2. 安装 Node.js 与 npm
3. 安装 pi CLI：

```bash
npm install -g @mariozechner/pi-coding-agent
```

并确保 `pi --version` 可用。

另外需要你本地可用的模型认证（任选其一）：
- 设置 API Key 环境变量（如 `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`）
- 或先在终端执行 `pi` 并完成 `/login`

## 启动

```bash
npm install
npm run tauri dev
```

## 关键文件

- `src/App.tsx`：UI + 监听 `pi://stream` + 调用 `invoke`
- `src-tauri/src/lib.rs`：
  - `stream_pi_prompt`（解析 RPC JSON 事件）
  - `abort_pi_stream`（中止当前生成）
  - `clear_pi_session`（清理会话文件）
- `src-tauri/tauri.conf.json`：Tauri 2.0 配置

## Agent Workspace

NineClaw 现在把智能体拆成两层：

- 数据库负责索引、模型、技能、执行模式
- `~/.nineclaw/workspace/agents/<agent-id>/` 负责该智能体自己的 `IDENTITY.md`、`ROLE.md`、`MEMORY.md`、`TOOLS.md`、`WORKING.md` 等文件

相关说明：

- `docs/AGENT_WORKSPACE_ARCHITECTURE.md`
- `docs/AGENT_CREATION_FLOW.md`

## 注意

- 当前实现依赖系统 PATH 中存在 `pi` 命令
- 当前方案是“每次请求启动一个 RPC 子进程”，但共享同一个 session 文件以保留上下文
- 当前展示流式文本输出（`text_delta`），后续可扩展工具调用事件与更细粒度状态
