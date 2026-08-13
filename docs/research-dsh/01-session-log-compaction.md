# 报告 01：会话日志真相源 + Compaction + TokenMeter

> 对照基准：dsh 的 append-only 事件日志（model-visible ⟺ logged）、tool-result 码点裁剪、compaction 锁事件、脱离式 TokenMeter。
> **可抄度：🟢 高**（tool-result 裁剪、配对断言、锁事件都是纯函数/局部改动，不碰 PI 内部）
>
> **实施状态**：步骤 ①、② 已于 2026-08-13 落地；完整状态见 [ROADMAP.md](./ROADMAP.md)。下文"当前态"仍保留调研时快照。

---

## 0. TL;DR

| # | 动作 | 改哪 | 抄自 dsh | 工作量 | 风险 |
|---|------|------|---------|--------|------|
| ① | tool-result 改 Unicode 码点 head/middle/tail 裁剪，替换 head-only + UTF-16 slice | `session_compression.rs` / 前端 `message-pair-utils.ts` | `pruneContent` | 半天 | 低 |
| ② | 压缩前后做 tool-call/result 配对平衡断言 | `session_compression.rs` | `toolPairingBalancedBefore/After` | 半天 | 低 |
| ③ | 压缩包 `compaction/start` + `compaction/end` 锁事件，启动检测孤儿锁 | `session_compression.rs` + PI session.jsonl 写入 | `compaction/*` 三事件 | 1 天 | 中 |
| ④ | 脱离式 TokenMeter：读 PI session.jsonl 给出压力快照 + 逐条估价 | `pi_usage.rs` | `TokenMeter.measure()` | 1-2 天 | 中 |
| ⑤ | （长期）在 PI session.jsonl 上建投影+审计层，逼近"日志即真相源" | 新模块 | `deriveMessages` + 不变量 | 1 周+ | 高 |

---

## 1. 当前态

### 1.1 会话日志：四套数据，无一是模型真相源

nine-claw 维护**三套**日志，**加上 PI 的第四套**，彼此独立、无一致性保证：

| 数据 | 位置 | 性质 | 是模型上下文源？ |
|------|------|------|----------------|
| `session_llm_log` | `.debug/session-logs/{id}.md`（`session_llm_log.rs:230`） | append-only Markdown 审计兜底 | ❌ |
| `llm_trace` | `.debug/YYYY-MM-DD.jsonl`（`llm_trace.rs:294`） | 结构化 JSONL 调试追踪，受 `llm_trace_enabled` 开关 | ❌ |
| `chat_history` | SQLite `chat_sessions`/`chat_turns`（`chat_history.rs:6`） | 前端 UI 数据源，**允许原地 `update_chat_turn`**（`chat_history.rs:414`） | ❌ |
| **PI session.jsonl** | `/tmp/nineclaw-pi-session-{key}.jsonl`（`lib.rs:2593`） | **PI 自己写、自己投影成模型 messages** | ✅（黑盒） |

**关键事实**：nine-claw 只通过 stdin 发**单条** prompt（`{type:"prompt",message,images}`，`send_pi_prompt_command` `lib.rs:1949`），不投影上下文。PI 内部如何把 session.jsonl 组装/裁剪/注入成最终 messages，nine-claw 完全不感知。

> 因此 dsh 的铁律"model-visible ⟺ logged"在 nine-claw 里**当前既不存在也无法直接存在**——绕过日志的路径就是 PI 本身。这是报告的核心约束。

### 1.2 压缩：insert_then_compress，让 PI 自己摘要

流程（`maybe_compact_desktop_session_with_plan` `lib.rs:2121`）：
1. nine-claw 读 PI session.jsonl（`load_session_entries` `session_compression.rs:81`）→ `plan_compression` 规划（`session_compression.rs:93`）
2. 给 PI 发 `{type:"compact",customInstructions}` RPC（`lib.rs:2140`）→ **PI 做 LLM 摘要**
3. PI 返回后，nine-claw 写 chunk archive `.md` + 追加一条 `type:"compaction"` 条目（`append_compaction_entry` `session_compression.rs:236`）

触发（`CompressionConfig` `session_compression.rs:18`）：token 阈值（默认 150k，来自 provider usage）/ 消息数（200）/ idle（90s）/ 模型切换。

**tool-result 裁剪现状（这是最该先抄的）**：
- `select_recent_with_tool_pairs`（`session_compression.rs:362`）保留了 call/result 配对 ✓
- 但归档裁剪 `truncate`（`session_compression.rs:614`）**只取 head**：`value.chars().take(max)`
- 前端 `truncateToolResult`（`message-pair-utils.ts:124`）用 `.slice(0,2000)` —— **UTF-16 码元，会劈开 surrogate pair（emoji/中文扩展字符）**
- `hasBrokenToolPairs`（`message-pair-utils.ts:68`）**生产代码无调用方**（仅有 `compressor.test.ts` 的测试引用）

**没有** compaction 锁事件：压缩中途崩溃（如 compact RPC 成功但 archive 写入失败 `lib.rs:2153`）会留下不可检测的不一致。idle 压缩只用 epoch 计数器防重入（`lib.rs:2188`），非事务性锁。

### 1.3 Token 计量：纯被动，无脱离式测量

`pi_usage.rs` 全是解析/累加 provider 返回的 usage（`extract_usage_payload` `pi_usage.rs:61`）。能拿到"当前压力"的唯一途径是**上一轮 LLM 返回的 usage**（`pooled.last_usage` `lib.rs:4237`）——历史值，非当前投影值。token 估算 `chars/4`（`estimate_entry_tokens` `session_compression.rs:320`）。

**致命点**：provider 不返回 usage 时（部分 provider/模型不返回），`maybe_compact_desktop_session_after_turn` 里 `usage` 为 None 直接 return（`lib.rs:2098`），**整个压缩触发链失效**。

---

## 2. dsh 做法（对照）

### 2.1 tool-result pruning（`dsh-compaction-tool-result-pruner`）

```ts
// 按 Unicode code point 量取，非 UTF-16；head/middle/tail 保留顺序
measureContent(blocks): number              // 非文本块计 0
pruneContent(blocks): ContentBlock[] | null // 过预算才裁，返回 null 表示无需裁
```
- 文本按 **Unicode 码点**切片，**不劈开 surrogate pair**（grapheme cluster 仍可能被切，可接受）
- 每个替换 cite 原 seq，保留完整事件数据只换 `content`
- 前面追加 `compaction/prune` 影子定价事件，纯消费者可直接减

### 2.2 配对平衡

```ts
toolPairingBalancedBefore(session, seq): boolean
toolPairingBalancedAfter(session, seq): boolean
```
压缩边界必须两侧都平衡，保证 assistant 的 tool-call 始终配对 result。校验当前 surface 成员资格，拒绝缺失 seq 和孤儿 result。

### 2.3 compaction 锁事件

| 事件 | 作用 |
|------|------|
| `compaction/start` | 先落，acquire 日志记录的锁 |
| `compaction/summary` | summary + shadowedRange + shadowedSeqs + tokenCount |
| `compaction/end` | **最后**落，释放锁 |

> 锁包住**整个**操作。**最后**才写 `end`，使崩溃留下"有 start 无 end"的**可检测孤儿锁**，而非一个谎称完成的 `end`。

### 2.4 TokenMeter（脱离式）

```ts
measure(session, requestHeader?): TokenMeasurement
// 返回不可变快照：logRevision + totalTokens + surfaceTokens + nodes:[{seq,tokens}]
// O(surface)，每次 clone 节点
```
- `baseline.kind==='usage'`：复用最近成功 provider 调用的 usage（仅当请求信封匹配且 ≥ 启发式锚点）
- `estimated`：无可用锚点时，用固定启发式对完整信封+表面定价
- 用途：**压缩触发器**——独立测量，不依赖 provider 是否回 usage

---

## 3. 分步改造方案

### 步骤 ① tool-result Unicode 码点 head/middle/tail 裁剪（P0，半天）

**后端** `session_compression.rs`：

```rust
/// 按 Unicode 码点裁剪过长文本：保留 head/tail，中间用省略标记。
/// 用 char_indices 切，绝不劈开 surrogate pair。
fn prune_text_codepoint(text: &str, max_chars: usize, head: usize, tail: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars { return text.to_string(); }
    let head_iter = text.chars().take(head);
    let tail_iter = text.chars().skip(count - tail);
    // 拼接 head + 省略标记 + tail
    // ...（用 char_indices 计算字节边界，避免重切）
}
```
替换 `truncate`（`session_compression.rs:614`）的调用点，归档时对 tool-result 文本用 `prune_text_codepoint`。

**前端** `message-pair-utils.ts:124` `truncateToolResult`：
```ts
// 错误：str.slice(0, 2000)  ← UTF-16 码元
// 正确：按码点
const chars = Array.from(str);          // 按 USV 码点展开
if (chars.length <= 2000) return str;
return chars.slice(0, head).join('') + '…[' + (chars.length) + ' chars]…' + chars.slice(-tail).join('');
```

**验收测试**（`#[cfg(test)]` + vitest）：
- 含 emoji（`\u{1F600}`）和 CJK 扩展 B 的文本裁剪后不出现孤头代理对（`\uD800`-`\uDFFF` 单独）
- 短于阈值的文本原样返回
- head/tail 段落各自不超预算

### 步骤 ② 配对平衡断言（P0，半天）

`session_compression.rs` 新增：
```rust
/// seq 之前 tool-call 与 result 是否配对平衡（用于压缩边界 start）
fn tool_pairing_balanced_before(entries: &[SessionEntry], idx: usize) -> bool
/// seq 之后是否平衡（用于边界 end）
fn tool_pairing_balanced_after(entries: &[SessionEntry], idx: usize) -> bool
```
在 `plan_compression`（`session_compression.rs:93`）选定 recent/archive 分界后调用，不平衡则向左/右收缩分界直到平衡。

**验收测试**：构造"assistant(tool_call) → tool_result → user"序列，断言任何把 tool_call 归档但保留 result 的分界被拒绝（现有测试 `plans_threshold_compression_and_preserves_tool_pairs` `session_compression.rs:694` 可扩展）。

### 步骤 ③ compaction 锁事件（P1，1 天）

在 `append_compaction_entry`（`session_compression.rs:236`）周围加锁：

```rust
// maybe_compact_desktop_session_with_plan 内
let lock_seq = append_compaction_lifecycle_entry(path, "compaction/start", &lock_payload)?;
// ... 发 compact RPC ...
// ... 写 archive + compaction/summary 条目 ...
append_compaction_lifecycle_entry(path, "compaction/end", &end_payload)?;
// 必须在所有副作用成功后写 end
```

启动时（PI session 加载路径，`load_session_entries` 调用点附近）扫描：发现 `compaction/start` 无匹配 `end` → 记 warning + 视为上次压缩未完成 + 决定是否回滚（archive 已写则保留但标记 `orphan`，或走 `quarantine_pi_session_file` `lib.rs:2722` 隔离）。

**关键纪律**（抄 dsh）：`end` 必须**最后**写。哪怕 summary 已写，archive 写失败也不写 `end`——让孤儿锁暴露问题，而非用 `end` 谎称完成。

**验收测试**：
- 模拟"start 后 panic / 进程被杀"→ 重启检测到孤儿锁并上报
- 正常流程 start/summary/end 三条按序落盘
- 锁期间第二个压缩请求被拒（busy）

### 步骤 ④ 脱离式 TokenMeter（P1，1-2 天）

`pi_usage.rs` 新增：
```rust
pub struct SessionPressureSnapshot {
    pub log_revision: usize,                   // 消费的 entry 数
    pub total_tokens: u64,                      // 当前请求+响应压力估计
    pub baseline: PressureBaseline,             // { kind: 'usage'|'estimated', ... }
    pub nodes: Vec<TokenSurfaceNode>,           // [{ seq, tokens }]
}
pub struct TokenSurfaceNode { pub seq: usize, pub tokens: u64 }

/// 读 PI session.jsonl，给当前压力一个脱离式快照。
/// 优先用最近成功 provider usage 做锚点（仅当请求信封匹配且 ≥ 启发式），
/// 否则用启发式对完整表面定价。O(entries)。
pub fn measure_session_pressure(entries: &[SessionEntry], last_usage: Option<&PiTokenUsagePayload>) -> SessionPressureSnapshot
```

改进点（相比 `chars/4`）：
- 分角色定价（system/user/assistant/tool 的 framing 不同）
- tool_result 按"内容块"而非整体计
- 可选引入 `tiktoken-rs`（BPE 真实分词，代价是加依赖；先用启发式，留 trait 扩展点）

**接入**：在 `maybe_compact_desktop_session_after_turn`（`lib.rs:2098`）里，当 provider 不返回 usage 时，**改用** `measure_session_pressure` 的估计值做触发，而不是直接 return——**修复"provider 不回 usage 就不压缩"的致命点**。

**验收测试**：
- 同一 entries 两次 measure 结果一致（确定性）
- 注入一条超长 tool_result 后 total_tokens 单调上升
- 锚点复用：有匹配 usage 时用 usage，否则 heuristic，且 heuristic ≥ 对应 usage 锚点

### 步骤 ⑤（长期，P3）日志投影+审计层

若要在 nine-claw 逼近"日志即真相源"：
- 把 PI session.jsonl（`type:"message"|"compaction"`）作为**唯一真相源**
- 写 `derive_model_messages(entries) -> Vec<Message>`：与 PI 的投影逻辑对齐（需逆向 PI 的裁剪/注入规则，或让 PI 暴露"当前发给模型的 messages"RPC）
- 在 nine-claw 的三套日志（session_llm_log / llm_trace / chat_history）与投影结果间加**一致性断言测试**（至少一个场景断言四套数据可互相校验）
- 风险高、依赖 PI 配合，建议单独立项，本报告仅占位。

---

## 4. 工作量与风险

| 步骤 | 工作量 | 风险 | 依赖 |
|------|--------|------|------|
| ① | 0.5 天 | 低（纯字符串函数） | 无 |
| ② | 0.5 天 | 低 | 与 ① 同文件 |
| ③ | 1 天 | 中（动 PI session.jsonl 写入路径，需小心 PI 自己也读这个文件） | 需确认 PI 对未知 entry type 的容忍度 |
| ④ | 1-2 天 | 中（启发式精度影响触发准确性） | 无 |
| ⑤ | 1 周+ | 高（需逆向/协商 PI 投影） | PI 配合 |

## 5. 边界与不建议

- **不建议**试图让 nine-claw 接管全部上下文投影而把 PI 降为无状态执行器——工作量巨大且打破现有 PI 集成契约。先做步骤 ①-④（局部、可逆、有测试），⑤ 仅作长期方向。
- 步骤 ③ 写 PI session.jsonl 前，**必须先验证 PI 遇到未知 `type`（`compaction/start`/`end`）不会崩溃或拒绝 replay**——PI 把它当 `serde_json::Value` 读（`session_compression.rs:88`），大概率容忍，但要在真机测一次。
