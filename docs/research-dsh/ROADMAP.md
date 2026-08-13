# research-dsh 实施路线图

统一追踪 4 份报告中的改造项。状态：`✅ 已落地` / `🟡 部分落地` / `⬜ 未开始` / `🚫 已否决`。

## 第一批已落地（2026-08-13）

| ID | 动作 | 来源 | 状态 | 备注 |
|----|------|------|------|------|
| 01-① | tool-result Unicode 码点 head/middle/tail 裁剪 | 报告 01 | ✅ | 后端 `prune_text_codepoint`；前端 `truncateToolResult` 改 `Array.from`，均有测试 |
| 01-② | 压缩边界 tool-call/result 配对平衡 | 报告 01 | ✅ | `tool_pairing_balanced_before/after` + `select_recent_with_tool_pairs_balanced` |
| 04-① | CI + 测试门禁 | 报告 04 | 🟡 | `.github/workflows/ci.yml` 已加 tsc/vitest/cargo test/800 行 ratchet；lint、clippy、rustfmt 因存量欠账暂未硬门禁 |
| 04-⑥ | 800 行机械校验 | 报告 04 | ✅ | `scripts/check-file-size.mjs` + 40 个存量超限文件的 ratchet baseline |

## 待办池

| ID | 动作 | 来源 | 优先级 | 备注 |
|----|------|------|--------|------|
| 01-③ | compaction start/summary/end 锁事件 + 孤儿锁检测 | 报告 01 | P1 | 需先真机验证 PI 对未知 entry type 的容忍度 |
| 01-④ | 脱离式 TokenMeter | 报告 01 | P1 | 修 provider 不回 usage 时压缩链失效 |
| 01-⑤ | PI session.jsonl 投影 + 审计层 | 报告 01 | P3 | 依赖 PI 配合，长期方向 |
| 02-① | 侦察 PI 扩展协议并沉淀能力清单 | 报告 02 | P2 | 解锁 02-②③ 的前置 |
| 02-② | JS `tool_call` pre 规则链 | 报告 02 | P2 | 受 02-① 约束 |
| 02-③ | post-execute 钩子 | 报告 02 | P2 | PI 不支持则记录缺口并放弃 |
| 02-④ | Rust 侧工具错误归因 | 报告 02 | P2 | 纯观测层，不碰 PI |
| 02-⑤ | 把 `run_guard_chain` 文档化为 waterfall 范本 | 报告 02 | P2 | 零代码 |
| 02-⑥ | `ToolExecutor` trait 空壳 | 报告 02 | P3 | 前瞻扩展点 |
| 03-① | Skills watcher + 缓存 | 报告 03 | P1 | 消灭每轮重扫盘 |
| 03-② | Skills 显式优先级 + provider 抽象 | 报告 03 | P1 | 与 03-① 合并做 |
| 03-③ | flat `<name>.md` + kebab 校验 | 报告 03 | P1 | |
| 03-④ | `execution_mode` 日志 fold | 报告 03 | P2 | 先做一个样板状态 |
| 03-⑤ | Scope 原语能力隔离 | 报告 03 | P3 | 依赖 02-② 的 PI 侧拦截才真正 enforce |
| 04-② | 临时文件 0700 + 随机名 + 0o600 | 报告 04 | P0 | 剩余 P0，建议下一批 |
| 04-③ | dispatcher `catch_unwind` | 报告 04 | P1 | |
| 04-④ | redact 安全默认 | 报告 04 | P0 | 剩余 P0，建议下一批 |
| 04-⑤ | emit 统一吞错 + log | 报告 04 | P1 | |
| 04-⑦ | clippy 纪律 | 报告 04 | P1 | 当前 142 个存量错误，需逐条 allow/fix |
| 04-⑧ | coverage gate / snapshot / real-API e2e 自 skip | 报告 04 | P1 | |
| 04-⑨ | postmortem 模板 + ADR | 报告 04 | P2 | |
| 04-⑩ | 端到端 trace id | 报告 04 | P2 | |

## 建议下一批

1. `04-②` 临时文件安全
2. `04-④` redact 安全默认
3. `04-⑤` emit 统一吞错 + log
4. `03-① + 03-② + 03-③` Skills 发现优化（一组做）
5. `01-③ + 01-④` compaction 可靠性
