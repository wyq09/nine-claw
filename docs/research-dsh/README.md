# DeepSeek Harness (dsh) 抄作业调研报告

本目录是把开源项目 [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)（10.5k⭐，纯 TypeScript 的通用 agent harness 框架，"一切皆插件"）的设计经验，**对照 nine-claw 现有代码**拆成的 4 份可执行调研报告。

每份报告独立、可单独落地，结构统一为：**当前态（带 `file:line`）→ dsh 做法 → 差距分析 → 分步改造方案（含要改的文件/函数签名/验收测试）→ 工作量与风险**。

## 报告索引

| # | 报告 | 核心主题 | 可抄度 |
|---|------|---------|--------|
| 01 | [会话日志真相源 + Compaction + TokenMeter](./01-session-log-compaction.md) | tool-result 码点裁剪 / 压缩锁事件 / 脱离式 token 测量 | 🟢 高（多块纯函数可抄） |
| 02 | [工具执行流水线 + 能力 Seam + 错误归因](./02-tool-pipeline-seam.md) | before/around/after 中间件 / 拒绝≠失败 | 🟡 中（受 PI 托管约束） |
| 03 | [多 agent Scope 隔离 + Skills 发现 + 日志 fold 状态](./03-multiagent-skills.md) | Scope 原语 / 优先级发现 / watcher / 状态 fold | 🟢 高（nine-claw 自有模块） |
| 04 | [防御模式 + 测试分层 + Postmortem/ADR + 工具链](./04-engineering-discipline-testing.md) | 临时文件权限 / CI 门禁 / snapshot / 文化基建 | 🟢 最高 ROI |

| — | [实施路线图与状态追踪](./ROADMAP.md) | 所有改造项的状态、批次和依赖 | 📋 |
| — | [dsh 对照基准快照](./dsh-reference.md) | 上游 commit、nine-claw commit、变更记录 | 📌 |


## 贯穿全局的关键约束（必读）

调研后浮现的一个**架构事实**决定了每份报告的可行性边界：

> **nine-claw 把整个 agent loop（工具选择、工具执行、上下文组装）完全委托给外部 PI 二进制。** Rust 层只是 PI 的"宿主/UI 外壳"——负责进程生命周期、stdin/stdout JSON 传输、事件转发、日志。**工具从不经过 Rust**，模型上下文来自 PI 自己的 `session.jsonl`，而非 nine-claw 维护的任何日志。

这带来三个直接推论：

1. **dsh 的"会话日志即唯一真相源（model-visible ⟺ logged）"无法在 nine-claw 一比一复刻**——因为模型实际看到什么由 PI 黑盒决定。报告 01 给出了"在 PI session.jsonl 上建立投影+审计层"的折中方案。
2. **dsh 的工具执行 waterfall（pre/around/post）无法搬到 Rust**——工具根本不经过 Rust。报告 02 把唯一可行的 seam 锁定在 JS 扩展（`managed_runtime_extension.rs` 的 `pi.on("tool_call")`），并诚实标注受 PI 扩展协议约束。
3. **报告 03、04 受 PI 约束最小**——skills 发现、多 agent 编排、工程纪律都是 nine-claw 自有领地，可抄度最高。

## dsh 核心思想速览（4 份报告共用背景）

- **Cordis 插件树**：没有特权内核，扩展方式是把插件挂到其他插件旁边；注册是**可逆副作用**，卸载即回滚。（Rust 里用 trait + 注册表 + RAII 等价）
- **能力 Seam 三角色**：Service Definition（接口）+ Provider（实现）+ Consumer（使用）。替换一个 Provider 改变整个产品。
- **事件四模式**：`emit`（观察）/ `waterfall`（around 中间件，必须调 `next()` 委托）/ `parallel`（扇出）/ `serial`（有序）。
- **model-visible ⟺ logged**：到达模型请求的一切必须能从日志重建。
- **防御模式 6 条**：见报告 04。

## 落地优先级建议

| 优先级 | 动作 | 报告 | 工作量 |
|--------|------|------|--------|
| **P0** | CI 门禁 + 800 行机械校验 + clippy + 临时文件权限 + redact 安全默认 | 04 | 1-2 天 |
| **P0** | tool-result 码点裁剪（替换会劈 surrogate pair 的 slice） | 01 | 半天 |
| **P1** | Skills watcher + 缓存 + 优先级表（消灭每轮重扫盘） | 03 | 2-3 天 |
| **P1** | 压缩锁事件 + 配对平衡断言 + 脱离式 TokenMeter | 01 | 2-3 天 |
| **P1** | 测试分层：snapshot / coverage gate / real-API e2e 自 skip | 04 | 3-5 天 |
| **P2** | JS 工具 pre-hook 升级为可组合规则链 + Rust 错误归因 | 02 | 2-3 天（需先侦察 PI 协议） |
| **P2** | 日志 fold 式软状态（execution_mode 翻转可恢复） | 03 | 2-3 天 |
| **P3** | Scope 原语做能力隔离（替换 prompt 软约束） | 03 | 1 周+（架构级） |
| **P3** | Postmortem/ADR 文化基建 + 端到端 trace id | 04 | 持续 |

> 最新落地状态以 [ROADMAP.md](./ROADMAP.md) 为准。所有改动须遵循 `AGENTS.md`：**改前先写单测、覆盖所有修改到的功能、全部测试通过才算完成**；手写源码单文件不超 800 行（存量超限文件用 `scripts/check-file-size.mjs` 的 ratchet baseline 管理，只许拆不许涨）。
