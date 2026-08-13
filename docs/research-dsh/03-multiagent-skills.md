# 报告 03：多 agent Scope 隔离 + Skills 发现 + 日志 fold 状态

> 对照基准：dsh 的 Scope 原语（不透明 key → per-agent 可见性 + 共享生命周期，分层 Registry）、Skills 分层 Provider + 优先级表 + watcher + 变更事件、日志 fold 式软状态。
> **可抄度：🟢 高**——skills / 多 agent 编排 / 状态都是 nine-claw 自有领地，受 PI 约束最小。

---

## 0. TL;DR

| # | 动作 | 改哪 | 抄自 dsh | 工作量 |
|---|------|------|---------|--------|
| ① | Skills 加 `notify` watcher + 缓存 + 失效，**消灭每轮重扫盘** | `skills.rs` + `skill_broker.rs` | chokidar watcher + `skills/change` | 2-3 天 |
| ② | Skills 发现改**显式数值优先级表 + provider 抽象**，替代隐式插入顺序 | `skills.rs` | rank 100-600 + `SkillProvider` trait | 1-2 天 |
| ③ | Skills 支持 flat `<name>.md` + kebab-case 强制校验 | `skills.rs` | flat/bundle 双格式 | 半天 |
| ④ | 日志 fold 式软状态：execution_mode 等翻转做成可恢复的 session 事件 | `managed_runtime.rs` SessionEventKind | `plan/mode` fold | 2-3 天 |
| ⑤ | 引入 Scope 原语做能力隔离（替换 prompt 软约束） | 新模块 + `agents.rs` | `Scope`/`ScopedLayers` | 1 周+（架构级） |

---

## 1. 当前态

### 1.1 多 agent：星型委派，靠"配置列 + prompt 软约束"隔离

- 模型：**supervisor 协调多个 member** 的星型拓扑（非 mesh）。成员间不直接通信，跨成员路由经两条渠道：
  1. **应用内委派**（`team_workspace.rs`）：supervisor 回复里输出 `NINECLAW_DELEGATE_JSON:` / `NINECLAW_DELEGATE_PLAN_JSON:` 标记（`team_workspace/delegate_markers.rs:30/54`），后端扫描后**为每个 assignee 起独立 PI 子进程**（`run_delegate_with_provider_events` `team_workspace.rs:505`）
  2. **HTTP 对等网关**（`peer_gateway.rs`）：axum 监听 `0.0.0.0:1052`，按 `to_agent_id` 路由（`peer_gateway.rs:750`）
- **隔离方式**：每个 agent 记录自带配置 blob（`agents` 表列）+ 每次调用是全新 PI 进程。**无 Scope 原语、无分层 Registry**（全仓 grep `struct.*Scope`/`ScopedLayer`/`ToolRegistry` 为空）
- 并发：`scheduler/mod.rs:48 MAX_PARALLEL_RUNS=2`；委派句柄存 `OnceLock<Mutex<HashMap<String,PiRunHandle>>>`（`team_workspace.rs:150`）做 abort
- **资源归属全在 `agents` 表列 或 `agents/<id>/` 文件目录**，无"全局层 vs per-agent 层"运行时分层

### 1.2 能力/资源归属：靠 prompt 软约束，非能力层 guard

- 工具：`allowed_tool_ids` 列 → `restrict_harness_to_agent_tools`（`managed_runtime.rs:564`）物化成 per-invocation `harness-effective-*.json`
- skills：`skill_ids` + `capability_policy` 列 → 每轮 broker 选完 → `--skill <path>`（`lib.rs:4978`）
- **memory 隔离靠 prompt**：一段硬编码 `--append-system-prompt`「记忆隔离规则：…禁止读取、引用、总结或迁移其他智能体目录下的记忆文件」（`lib.rs:4927`、`pi_bridge.rs:1540`）——**不是能力层 guard，模型不听话就漏**
- 无"注册临时工具→用完清理"的 scope 生命周期；要改工具集只能 `update_agent` 改 DB 行（持久化）

### 1.3 Skills：单一本地 FS provider，每轮重扫盘（重点优化项）

- **单一 provider**（本地文件系统），无 provider 抽象、无远程
- 发现根（`skills.rs:663 candidate_skill_roots`），**隐式插入顺序即优先级**，first-seen-wins 去重（`skills.rs:519`）：

  | 序 | 目录 | scope 标签 |
  |----|------|-----------|
  | 1 | `<workspace>/skills/` | workspace |
  | 2 | `<workspace>/.agents/skills/` | workspace |
  | 3 | `$CODEX_HOME/skills` | global |
  | 4 | `~/.codex/skills` | global |
  | 5 | `~/.agents/skills` | global |
  | 6 | bundled `system-skills` | runtime |

- **仅 bundle 格式** `<id>/SKILL.md`（`skills.rs:514`），**无 flat `<name>.md`**；skill_id=目录名，**无 kebab-case 强制校验**
- **无 watcher、无缓存**：全仓 grep `notify::`/`watcher`/`debounced` 为空。每次 `select_skills_for_turn`（`skill_broker.rs:23`）→ `list_runtime_available_skills`（`skills.rs:101`）→ **对所有 root 跑 `fs::read_dir` 逐轮重扫**（`scan_skill_definitions_from_roots` `skills.rs:579`）
- **无 `skills/change` 事件**；最接近的 `emit_pi_stream_skill_selection_event`（`lib.rs:4963`）是"本轮挂了哪些 skill"的观测事件，非"目录变了请重扫"信号
- broker 打分：static/hybrid/dynamic 策略（`agent_capabilities.rs:87`），trigger/example 短语命中 +20、偏好 skill +24 等（`skill_broker.rs:216 score_skill`），取 top-N

### 1.4 状态持久化：几乎全在 SQLite，无日志 fold

- 模式/开关/policy 几乎全落 `agents` 表列 或 `app_state` KV 表（`agents.rs:560` UPDATE / `peer_gateway.rs:239`）
- `runtime_parameters`（max 轮数等）在**进程内 `static RwLock`**（`runtime_parameters.rs:9`），**崩溃不恢复**，每次从前端重新 merge
- `agent_presets` 是**编译期 `include_str!` 静态 JSON**（`agent_presets.rs:9`，6 个内置），是创建 agent 的工厂模板，非 per-session 状态
- **有 append-only 会话事件日志**（`SessionEventKind` `managed_runtime.rs:38` = Prompt/ToolCall/ToolResult/AssistantOutput/Decision/RuntimeError/RuntimeRetry，`append_session_event` `:620`），**但没有 Mode/Plan 类 event kind，也没有任何 fold**——事件读回（`:795`）只用于审计/展示，不重建当前状态

---

## 2. dsh 做法（对照）

### 2.1 Scope 原语

- `ScopeKey` = 不透明对象身份（live Agent 对象）
- 注册分**全局层 + 精确 scope 层**；读取 `merge()` = 全局层 + 最近 scope 层链，**最近 scope 层对重名直接胜出**
- `ScopedLayers<L>`：eager 全局层 + 懒创建 exact-scope 层；**scope 完全清空时回收**层
- 注册用同一个 context 表达"可见性 + 生命周期"，返回精确 disposer，scope 清空自动回收 → 天然支持"本次调用临时挂工具，调用结束自动回收"

### 2.2 Skills 分层 Provider

- 多 provider（local/packaged/remote）合并；注册同步，远程发现放 `list()`
- **显式数值优先级表**：project `.dsh/skills`(100) > `.agents/skills`(200) > custom(300) > user-dsh(400) > user-agents(500) > bundled(600)
- 重名裁决：rank → provider order → local order
- flat `<name>.md` + bundle `<name>/SKILL.md` 双格式；kebab-case 强制
- chokidar 监听根目录；**模型 write/edit 命中 catalog 路径时同步失效 provider 缓存**
- `skills/change` 事件（无 diff）通知消费者重新 `snapshot()`

### 2.3 日志 fold 式软状态

`plan/mode` = log-only 整值替换事件，`foldPlanMode(events)` 返回最后 logged 值 → 当前状态 = 会话日志的**纯 fold**。resume/fork/compaction **自动恢复**，无需内存镜像。UI 通过 `session/event` 观察翻转。

---

## 3. 分步改造方案

### 步骤 ① Skills watcher + 缓存（P1，2-3 天，性能大杀器）

**问题**：当前每个 agent 每轮对话都 `fs::read_dir` 扫所有 skill 根目录（`skills.rs:579`），workspace skill 多时是真实开销。

**方案**：引入 `notify` crate（跨平台 FS 监听，Rust 版 chokidar）：

```rust
// skills.rs 新增
struct SkillCache {
    entries: RwLock<HashMap<SkillRoot, Vec<SkillManifest>>>,  // 按 root 缓存
    invalidated: AtomicBool,
}

fn start_skill_watchers(roots: &[PathBuf], cache: Arc<SkillCache>, app: AppHandle) {
    // 对每个存在的 root 起一个 notify watcher（Recommended debouncer 200ms）
    // 事件：Create/Remove/Modify 命中 SKILL.md 或 *.md → invalidate 对应 root
    // 失效后 emit "skills/change" Tauri 事件，通知前端/重算
}
```

- `list_runtime_available_skills` 先读缓存，miss 或 invalidated 才扫盘
- 写失效：模型 write/edit 命中 skill 目录（在 `managed_runtime_extension.rs` 的 fs 工具里识别）→ 同步标 invalidated（抄 dsh "write/edit 命中 catalog 路径时失效"）
- 目录不存在时，watcher 从最近存在的祖先目录起，按缺失段增量跟随（抄 dsh 的"absent path 逐段跟随"）

**验收测试**（`#[cfg(test)]`，用 `tempfile` + 短 debounce）：
- 新增 `<root>/new-skill/SKILL.md` → 下一轮 `list` 含它，无需重启
- 删除 skill → 下一轮不含
- 改 manifest → 下一轮读新内容
- watcher 异常时 `list` 仍能直接读盘兜底（不静默隐藏可用 skill，抄 dsh"watcher 失败使 observation incomplete 但不藏候选"）

### 步骤 ② 显式优先级表 + provider 抽象（P1，1-2 天）

`skills.rs` 引入 `SkillProvider` trait + 数值 rank：

```rust
trait SkillProvider: Send + Sync {
    fn name(&self) -> &str;
    fn rank(&self) -> u32;                              // 100/200/.../600
    async fn list(&self, opts: &SkillLookupOptions) -> Vec<SkillCandidate>;
    async fn get(&self, id: &str) -> Option<SkillDefinition>;
}

// 内置实现
struct WorkspaceSkillProvider { rank: 100, root: workspace/skills }
struct AgentsSkillProvider    { rank: 200, root: workspace/.agents/skills }
struct CodexHomeProvider      { rank: 300, root: $CODEX_HOME/skills }
struct UserCodexProvider      { rank: 400, root: ~/.codex/skills }
struct UserAgentsProvider     { rank: 500, root: ~/.agents/skills }
struct BundledProvider        { rank: 600 }
```

- 重名裁决改 `rank → provider order → local order`（替代 first-seen-wins）
- 现有 `skill_scope_rank`（`skills.rs:921`）当前只用于**展示排序**——升级为**决定胜出**的真实 rank
- 留 trait 扩展点为未来"远程 skill provider"铺路（dsh 的 remote discovery 放 `list()`）

**验收测试**：同名 skill 在 workspace(100) 和 user(400) 都存在 → 选 workspace 的；同 rank 时按 provider 注册顺序。

### 步骤 ③ flat `<name>.md` + kebab 校验（P0，半天）

`skills.rs` 扫描（`scan_skill_definitions_from_roots` `:579`）：
- 除 `<id>/SKILL.md` 外，接受 `<name>.md`（flat，frontmatter 即 manifest，正文为 body）
- skill_id 解析后用正则 `^[a-z0-9]+(?:-[a-z0-9]+)*$` 校验，不合规 → 跳过 + warning（不 crash）

**验收测试**：`prompt-engineer.md`（flat）与 `prompt-engineer/SKILL.md`（bundle）都被发现；`Prompt Engineer.md`（含空格大写）被跳过并告警。

### 步骤 ④ 日志 fold 式软状态（P2，2-3 天）

让 `execution_mode`（agent 的执行模式开关）这类**软引导状态**做成可恢复的 session 事件 fold，而非纯 SQLite 列 + 进程内 static。

`managed_runtime.rs` 扩展 `SessionEventKind`：
```rust
enum SessionEventKind {
    // ... 现有 ...
    ModeSet { mode: ExecutionMode },   // 整值替换事件
}
```

新增 fold：
```rust
/// 当前模式 = 会话事件流的纯 fold（取最后一条 ModeSet，无则默认）
fn fold_execution_mode(events: &[SessionEvent], end: Option<usize>) -> ExecutionMode
```

接入：
- `set_execution_mode(agent, mode)` 时**同时** append `ModeSet` 事件（DB 列可保留做快速读，但**事件流是权威**）
- resume/fork 时 `fold_execution_mode` 重建，保证崩溃后一致
- UI 通过现有 session 事件流观察翻转

**收益**：模式翻转成为可审计、可重放、崩溃可恢复的状态（而非"改了 DB 列就持久改了，进程内 static 却丢了"的不一致）。先把 **execution_mode** 一个状态做通作为样板，后续 policy 翻转照搬。

**验收测试**：
- append 3 条 `ModeSet`（A→B→A）→ fold 结果是 A
- 空事件流 → fold 返回默认
- fork 会话 → 子会话 fold 出与 fork 点一致的 mode

### 步骤 ⑤ Scope 原语做能力隔离（P3，1 周+，架构级）

**问题**：memory 隔离当前是 prompt 软约束（`lib.rs:4927`），模型不听话就漏。多 agent 并发时各 agent 能力集彼此独立但靠"各自从 DB 行派生"，无统一隔离抽象。

**方案**（大改，建议单独立项）：引入 Scope 原语：
```rust
struct ScopeKey(object);  // 不透明，用 AgentId 或 live handle
struct ScopedRegistry<V> {
    global: RwLock<HashMap<String, V>>,
    scopes: RwLock<HashMap<ScopeKey, HashMap<String, V>>>,
}
impl ScopedRegistry {
    fn register(&self, key: Option<ScopeKey>, name, val) -> ScopeGuard;  // RAII，drop 回收
    fn merge(&self, key: Option<ScopeKey>) -> Vec<(String, V)>;          // scope 层覆盖全局
}
```
- 工具/skills/能力注册落到调用 context 的 scope 层
- "给 agent X 临时挂工具 Y，用完自动回收"变成 `registry.register(Some(agent_x), "Y", tool).await?` + guard drop
- **memory 隔离从"prompt 告诉模型别读"升级为"scope 决定 fs 工具的可见根"**（fs 工具读路径时查 scope 的允许根集合，越界直接拒，不靠模型自觉）

**注意**：fs 工具在 PI 内执行，真正 enforcement 仍受报告 02 的 PI 约束——需配合报告 02 的 JS pre 规则链（步骤 ②）在 PI 侧拦截越界路径。本步在 Rust 侧建好 scope 数据模型与策略解析，JS 侧消费。

**验收测试**：
- agent X scope 注册工具 Y，agent Y 的 `merge` 不含 Y
- guard drop 后，全局 `merge` 不再含该临时工具
- 越界路径访问被 scope 策略拒绝（配合 JS 拦截）

---

## 4. 工作量与风险

| 步骤 | 工作量 | 风险 | 阻塞 |
|------|--------|------|------|
| ① watcher+缓存 | 2-3 天 | 中（debounce 时序、跨平台 notify 行为差异需测） | 无 |
| ② 优先级表+provider | 1-2 天 | 低 | 与 ① 同文件，建议合并一轮做 |
| ③ flat+kebab | 0.5 天 | 低 | 无 |
| ④ 日志 fold 状态 | 2-3 天 | 中（需保证 DB 列与事件流不长期漂移） | 无 |
| ⑤ Scope 原语 | 1 周+ | 高（动隔离模型，需配合报告 02 的 JS 拦截才真正 enforce） | 报告 02 |

## 5. 边界与不建议

- **不建议**一次性把所有软状态都改成 fold——先把 `execution_mode` 一个做通验证模式，再推广。`runtime_parameters`（max 轮数）这类"前端每轮重新 merge"的状态本就无持久化需求，不必强行 fold。
- **不建议**为了抄 Scope 而重构整个 `agents` 表模型。Scope 是**新增的运行时隔离层**，DB 列仍可保留做持久化默认值；scope 层是会话内的、临时的、可回收的。两者职责不同，不要合并。
- ⑤ 的 memory 隔离 enforcement **依赖报告 02 的 PI 侧拦截**——纯 Rust 建 scope 模型而不在 PI/JS 侧消费，模型仍能读到越界路径。务必和报告 02 步骤 ② 一起规划，否则只是"数据模型对了但没拦住"。
- skills watcher 是本报告**性价比最高**的一项：纯 nine-claw 自有、不碰 PI、直接消灭每轮重扫盘的性能问题，且有清晰验收。建议优先做 ①②③ 一组。
