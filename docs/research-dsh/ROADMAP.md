# research-dsh 实施路线图

统一追踪 4 份报告中的改造项。状态：`✅ 已落地` / `🟡 部分落地` / `⬜ 未开始` / `🚫 已否决`。

## 第一批已落地（2026-08-13）

| ID | 动作 | 来源 | 状态 | 备注 |
|----|------|------|------|------|
| 01-① | tool-result Unicode 码点 head/middle/tail 裁剪 | 报告 01 | ✅ | 后端 `prune_text_codepoint`；前端 `truncateToolResult` 改 `Array.from`，均有测试 |
| 01-② | 压缩边界 tool-call/result 配对平衡 | 报告 01 | ✅ | `tool_pairing_balanced_before/after` + `select_recent_with_tool_pairs_balanced` |
| 04-① | CI + 测试门禁 | 报告 04 | 🟡 | `.github/workflows/ci.yml` 已加 tsc/vitest/cargo test/800 行 ratchet；lint、clippy、rustfmt 因存量欠账暂未硬门禁 |
| 04-⑥ | 800 行机械校验 | 报告 04 | ✅ | `scripts/check-file-size.mjs` + 40 个存量超限文件的 ratchet baseline |

## 第二批已落地（2026-08-13）

| ID | 动作 | 来源 | 状态 | 备注 |
|----|------|------|------|------|
| 04-② | 临时文件 0700 私有目录 + 0o600 + 拒符号链接 | 报告 04 | ✅ | `runtime_paths.rs`：会话/摘要/runtime 迁到 app data 私有目录，旧 /tmp 迁移一次；共享 temp 目录不再被 chmod（macOS EPERM 回归测试） |
| 04-④ | redact 安全默认 | 报告 04 | ✅ | Rust `include_secrets: Option<bool>` 缺省 false；前端 `includeSecrets?` 缺省传 false，2 个 vitest 用例 |
| 04-⑤ | emit 统一吞错 + log | 报告 04 | ✅ | `emit_safe.rs` helper（mock_app 真实路径测试），12 个模块全部替换裸/吞错 emit |

## 第三批已落地（2026-08-13）

| ID | 动作 | 来源 | 状态 | 备注 |
|----|------|------|------|------|
| 03-① | Skills watcher + 缓存 | 报告 03 | ✅ | `skill_cache.rs`：per-root 懒扫描缓存 + notify watcher，dirty 失效 + `skills/change` 事件，watcher 挂了回退读盘 |
| 03-② | Skills 显式优先级 + provider 抽象 | 报告 03 | ✅ | `skill_providers.rs`：rank 100-600 + `SkillProvider` trait，rank 排序决定重名胜出 |
| 03-③ | flat `<name>.md` + kebab 校验 | 报告 03 | ✅ | 双格式发现 + kebab 强制 + flat 物化挂载；skills.rs 1175→1070 行 |

## 第四批已落地（2026-08-13）

| ID | 动作 | 来源 | 状态 | 备注 |
|----|------|------|------|------|
| 04-③ | dispatcher `catch_unwind` | 报告 04 | ✅ | `safe_task.rs`：`safe_spawn`（observer 吞 JoinError）+ `safe_thread_spawn`（catch_unwind），8 个 bot worker 线程 + panic 隔离测试 |
| 01-③ | compaction 锁事件 | 报告 01 | ✅ | `compaction_lock.rs`：start/end 生命周期事件（end 最后写）、孤儿锁检测、进程内 busy 互斥 |
| 01-④ | 脱离式 TokenMeter | 报告 01 | ✅ | `token_meter.rs`：usage 锚点校验 + 角色启发式；provider 不回 usage 时压缩触发链不再失效 |

## 第五批已落地（2026-08-14）

| ID | 动作 | 来源 | 状态 | 备注 |
|----|------|------|------|------|
| 04-⑦ | clippy 纪律 | 报告 04 | ✅ | 存量 149 警告 + 1 硬错误清零；`[lints.clippy]` 显式 allow 两噪音类；CI 新增 `cargo clippy --lib --tests -- -D warnings` 硬门禁 |

## 待办池

| ID | 动作 | 来源 | 优先级 | 备注 |
|----|------|------|--------|------|
| 01-⑤ | PI session.jsonl 投影 + 审计层 | 报告 01 | P3 | 依赖 PI 配合，长期方向 |
| 02-① | 侦察 PI 扩展协议并沉淀能力清单 | 报告 02 | P2 | 解锁 02-②③ 的前置 |
| 02-② | JS `tool_call` pre 规则链 | 报告 02 | P2 | 受 02-① 约束 |
| 02-③ | post-execute 钩子 | 报告 02 | P2 | PI 不支持则记录缺口并放弃 |
| 02-④ | Rust 侧工具错误归因 | 报告 02 | P2 | 纯观测层，不碰 PI |
| 02-⑤ | 把 `run_guard_chain` 文档化为 waterfall 范本 | 报告 02 | P2 | 零代码 |
| 02-⑥ | `ToolExecutor` trait 空壳 | 报告 02 | P3 | 前瞻扩展点 |
| 03-④ | `execution_mode` 日志 fold | 报告 03 | P2 | 先做一个样板状态 |
| 03-⑤ | Scope 原语能力隔离 | 报告 03 | P3 | 依赖 02-② 的 PI 侧拦截才真正 enforce |
| 04-⑧ | coverage gate / snapshot / real-API e2e 自 skip | 报告 04 | P1 | |
| 04-⑨ | postmortem 模板 + ADR | 报告 04 | P2 | |
| 04-⑩ | 端到端 trace id | 报告 04 | P2 | |

## 建议下一批

1. `03-④` `execution_mode` 日志 fold（先做一个样板状态）
2. `04-⑧` coverage gate（先松后紧 ratchet）
3. `04-⑨` postmortem 模板 + ADR（零代码）
4. `01-⑤` / `02-①` 依赖 PI 配合的长期项
