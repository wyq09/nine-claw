# 报告 04：防御模式 + 测试分层 + Postmortem/ADR + 工具链

> 对照基准：dsh 的防御模式 6 条、测试 5 层（Unit/Coverage gate/Real-API e2e/Snapshot/Web snapshot）+ 测试哲学（验证世界而非自述 / 测真实入口路径 / 真实实现优先于 mock）、Postmortem + ADR 文化、~60 个 verify-* 门禁 + oxlint/knip/jscpd。
> **可抄度：🟢 最高 ROI**——这是不碰 PI、纯 nine-claw 自有、且当前缺口最大（无 CI）的一块。
>
> **实施状态**：步骤 ①（CI 骨架，lint/clippy/rustfmt 待存量清理）和 ⑥（800 行 ratchet）已于 2026-08-13 落地；完整状态见 [ROADMAP.md](./ROADMAP.md)。

---

## 0. TL;DR

| # | 动作 | 抄自 dsh | 工作量 | ROI |
|---|------|---------|--------|-----|
| ① | **加 CI + 把测试/lint 绑进流水线**（当前完全无 CI，测试不进 build） | lefthook + CI gates | 1-2 天 | ⭐⭐⭐⭐⭐ |
| ② | 临时文件 0700 私有目录 + 随机名 + 0o600（当前共享 /tmp 固定前缀） | 防御模式 ⑥ | 半天 | ⭐⭐⭐⭐ |
| ③ | in-process dispatcher 统一 `catch_unwind` + log（当前回调裸奔） | 防御模式 ⑤ | 半天 | ⭐⭐⭐ |
| ④ | redact 改安全默认（include_secrets 默认 false） | 防御模式 ⑥ | 0.5 天 | ⭐⭐⭐⭐ |
| ⑤ | emit 错误统一吞 + log（当前部分裸 emit 会向上传播） | 防御模式 ⑤ | 0.5 天 | ⭐⭐⭐ |
| ⑥ | 800 行上限**机械化**（脚本数行，替代人/agent 自觉） | verify-* 门禁 | 半天 | ⭐⭐⭐ |
| ⑦ | clippy `[lints]` + Rust lint 纪律（当前全 crate 无 lint） | oxlint 精神 | 半天 | ⭐⭐⭐ |
| ⑧ | 测试分层：coverage gate / 真 snapshot / real-API e2e 自 skip / 真实入口路径 | 测试 5 层 | 3-5 天 | ⭐⭐⭐⭐ |
| ⑨ | Postmortem 模板 + ADR + 把 DEV_NOTES 升级 | postmortem/ADR 文化 | 持续 | ⭐⭐⭐ |
| ⑩ | 端到端 trace id（IM→pi→llm→回复） | 可观测性 | 1-2 天 | ⭐⭐⭐ |

---

## 1. 当前态

### 1.1 工程规范（已有亮点，但机械校验薄）

`AGENTS.md` 已相当成熟（明显是踩坑踩出来的）：
- ✅ 800 行上限（手写源码，例外列了 vendored/机器生成/锁文件）
- ✅ Code Review Rubrics（5 维度 1-10，pass≥7.0 且无维度≤3）
- ✅ "所有功能开发前必须设计单元测试，覆盖所有修改到的功能"
- ✅ 受保护文件 pi launcher + `.githooks/pre-commit`（59 行）每次提交无条件重写正确内容
- ✅ 机械硬约束：`tsconfig.app.json` `strict:true` + `noUnusedLocals/noUnusedParameters/noFallthroughCasesInSwitch/verbatimModuleSyntax`，由 `tsc -b` 强制

**缺口**：
- 800 行上限**无脚本去数**，全靠自觉
- Rubrics 是人工打分，"先写测试"是口号
- **Rust 侧全 crate 无任何 `#![deny/warn]` 或 `#[clippy::]`**，`Cargo.toml` 无 `[lints]`，无 clippy.toml/rustfmt.toml，构建脚本无 `cargo clippy`
- 唯一零散的同类测试：`src-tauri/tests/system_skill_team_supervisor_orchestrator.rs`

### 1.2 测试现状（广度可观，但完全不进流水线）

- 前端：76 个 vitest 文件，jsdom，配置 `vite.config.ts:9`（`include: src/**/*.test.{ts,tsx}`）
- Rust：**371 个 `#[test]`** 分布在 ~50 个 `#[cfg(test)]` 模块，测试风格有讲究（`session_compression.rs:653` 自带带 Drop 的 TempDir；`app_log.rs:238` 测 `read_file_tail` 跳过半行；`lib.rs:1097` 测 mime 魔数优先于扩展名）
- 集成测试仅 2 个：`system_skill_team_supervisor_orchestrator.rs` + `vector_memory_integration.rs`（19KB，真 in-memory SQLite 跑全链路，**真实实现不 mock**）
- **无 coverage gate**（无 `@vitest/coverage-*`，无 thresholds）
- **无 e2e**（无 playwright，无 real-API-skip-if-no-key 模式）
- **无真 snapshot**（无 `.toMatchSnapshot()`；styles 测试用脆弱的 `readFileSync + .toContain`）
- **无"测真实入口路径"**（前端测试全跑 vite 即时编译的 tsx，非 built 产物）

> **最致命**：`npm run build` = `build:lark-helper → prepare:pi-runtime → tsc -b → vite build`（`package.json:8`），**全程不跑 vitest 也不跑 cargo test**（cargo 编译只发生在 `npm run tauri build` / `build-macos` 链路）。AGENTS.md 那条"全部测试通过才算完成"完全靠自觉。

### 1.3 防御性编程现状（pi 生命周期是范本，临时文件/dispatcher 有明确缺口）

**已做对（dsh 风格的已有纪律）**：
- ① 独立结果分开上报：`wait_for_child_exit_with_trace` 返回 `ChildExitOutcome { status, timed_out }` 两独立字段（`lib.rs:2391`）✓
- ④ Dispose 等静止：`kill_desktop_pooled_pi`（`lib.rs:2483`）顺序 `close_pi_stdin → kill_child_with_trace → wait_for_child_exit_with_trace`，超时二次 force-kill + grace poll ✓
- ⑥(部分) 清洗 env：`scrub_anthropic_process_env`（`lib.rs:3681`）`env_remove` 掉 ANTHROPIC_*，key 用显式命名 env 注入 ✓
- symlink lstat：`agent_package.rs:77`/`skills.rs:402`/`pi_runtime.rs:345` 用 `symlink_metadata`，`agent_package.rs:79` 拒绝导出 symlink ✓

**具体隐患（抄作业切入点，带 file:line）**：

1. **临时文件共享 /tmp + 固定前缀 + 无权限限制**（防御模式 ⑥）。`lib.rs:2593` `temp_dir().join(format!("{PI_SESSION_FILE_PREFIX}{key}.jsonl"))`、`:2605` media、`:2609` summary，前缀固定（`nineclaw-bot-session-`），**全仓 grep 不到 `0o600`/`0o700`**。单用户桌面机风险低，但 daemon 模式（`--scheduler-daemon`，README:74）或多用户主机下是真实信息泄露。
2. **dispatcher/回调无系统化兜底**（防御模式 ⑤）。全 crate 仅 1 处 `catch_unwind`且是测试（`lib.rs:1083`）。bot worker 线程（wechat/lark/pi_bridge）的 `spawn(async move{...})` 无统一 panic 防护，一个坏回调 panic 可能静默毒化状态。
3. **emit 错误处理不一致**。部分 `let _ = app.emit(...)`（吞错续跑 `lib.rs:2171/5474/7559`），部分裸 `app.emit(...)`（`lib.rs:3944/3973`）让失败 Result 向上传到流式循环。
4. **redact 非安全默认**。`redact_agent_record`（`agent_package.rs:52`）受 `include_secrets: bool` 控制，但该参数是**必填 bool 而非默认 false**（`piClient.ts:835`，UI 传入 `AgentsView.tsx:227`）。
5. **poison 恢复 ad hoc**。`clear_poison()`（`lib.rs:1078`）只对 channel manager 单点做了自愈，没推广。

### 1.4 可观测性（LLM 维度已达 dsh 级，缺端到端 trace id）

- `llm_trace.rs`（907 行）append-only 按日切片 JSONL，记录 system/user/response/thinking/tool_calls（全量 args_json+result）/usage/status/duration，delegate 带 `parent_trace_id`——**足以重建一次 LLM 会话** ✓
- app 日志 `tauri-plugin-log`+`fern` 按自然日轮转，`app_log.rs:51` 路径穿越校验 ✓
- `dev_trace.rs` debug eprintln ✓
- **缺口**：无贯穿"IM 入站 → pi spawn → LLM → 回复"的端到端 trace id（`parent_trace_id` 只在 workspace 内关联）

### 1.5 工程文化（有 guardrails 亮点，CI/ADR 缺失）

- ✅ **亮点**：`docs/HISTORY_STORAGE_GUARDRAILS.md`（11 条不变量）+ `docs/HISTORY_RECOVERY_GUARDRAILS.md`（14 条不变量）——真正的 dsh 式"可遵守契约"文档，明确要求"新增历史落盘逻辑必须补崩溃恢复测试"
- ✅ postmortem 雏形：`docs/DEV_NOTES.md`（7 条"问题→根因→修复→教训"）
- ❌ `technical-notes/` 仅 1 个文件（`macos-node-sigkill.md` 9 行）
- ❌ 无正式 ADR
- ❌ git hook 单一用途（只恢复 pi launcher，不跑 lint/test）
- ❌ **无 CI**（无 `.github/`/`.gitlab-ci.yml`）
- ❌ 工具链只有 eslint（无 oxlint/knip/jscpd/lefthook）

---

## 2. dsh 做法（对照）

### 2.1 防御模式 6 条（血泪规则）

1. **独立结果分开上报**：timeout 和 exit0 可同时发生（trap 信号）；`timedOut`/`signal`/`exitCode` 各自独立，绝不把一个 flag 嵌在另一个分支
2. **公共契约两边都遵守**：实现收到多表示先归一化再过公共 API；在类型定义处文档化归一化契约
3. **异步状态≠同步状态**：`agent.status==idle` 不是某条消息的结果；多个排队 follow-up/steering 可共享一个 running 区间
4. **Dispose 等静止而非仅请求停止**：kill → await done；**先关监听/通知注册，再 kill**
5. **dispatcher 兜住回调异常**：用户 listener 抛错不能 reject 所在 promise 或饿死后续 listener；dispatch loop try/catch + log
6. **不给不可信输出喂 env/可预测路径**：spawn 清洗 `*KEY*`/`*SECRET*`/`*TOKEN*`/`*PASSWORD*`；临时文件 0700 私有目录 + 随机名 + 独占打开（`'wx'`, `0o600`）；symlink 用 lstat + unlink

### 2.2 测试分层

| 层 | 工具 | 内容 |
|----|------|------|
| Unit | vitest / `#[test]` | 每包 tests；每个 registry 有 HMR-safety 测试（dispose 后断言清理） |
| Coverage gate | `vitest --coverage` | **逐文件 100%** on `src`；未覆盖行常是死代码该删 |
| Real-API e2e | `test:e2e` | 有 key 跑真 provider；**无 key 自 skip**；烟雾测试 boot 真例子发一条 prompt 检查世界 |
| Snapshot | `test:snapshot` | keyless 锁外部行为；CI 强制 replay 模式只 diff 不写 |
| Web snapshot | `test:web` | Chromium 比对浏览器输出 |

### 2.3 测试哲学

- **验证世界而非自述**：e2e 重跑命令/重读文件，不在 agent 自己输出找关键词；断言未触碰文件 byte-identical
- **测真实入口路径**：跑 built 产物（非 tsx），tsx 会掩盖竞态/模块解析/吞掉的加载失败
- **真实实现优先于 mock**：只 mock LLM/网络/时钟，下游全真实
- **guard 只有真让它失败才算 guard**：引入回归看红，再 revert

### 2.4 文化

- **Postmortem**：incident 写"为什么流程放过了它"+ guardrails；执行摘要 30 秒可读
- **Agent Note (ADR)**：设计决策 + 被否方案
- **门禁**：lefthook + oxlint/knip/jscpd + ~60 verify-*（文档链接/类型 drift/catalog 生成）

---

## 3. 分步改造方案

### 步骤 ① CI + 测试门禁（P0，1-2 天，ROI 最高）

**问题**：371 个 Rust 测试 + 76 个前端测试写得不少，但**没有任何东西自动跑它们**。

**方案 A：GitHub Actions**（`.github/workflows/ci.yml`，若用 GitLab 则 `.gitlab-ci.yml`）：
```yaml
jobs:
  lint-test:
    runs-on: macos-latest   # nine-claw 是 macOS 优先（Tauri）
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: 22 }
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy, rustfmt }
      - run: npm ci
      - run: npm run lint
      - run: npm run test            # vitest run
      - run: cargo test --manifest-path src-tauri/Cargo.toml
      - run: cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
      - run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
      # build 可选（重，放单独 job 或 nightly）
```

**方案 B：lefthook 多 hook**（替代/补充单一 `.githooks/pre-commit`）：
```yaml
# lefthook.yml
pre-push:
  commands:
    vitest: { run: npm run test }
    cargo-test: { run: cargo test --manifest-path src-tauri/Cargo.toml }
    clippy: { run: cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings }
pre-commit:
  commands:
    # 保留现有 pi-launcher 恢复逻辑
    eslint-staged: { run: npx eslint $(git diff --name-only --cached 'src/*.ts' 'src/*.tsx') }
    fmt-check: { run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check }
```
> 注：现有 `.githooks/pre-commit` 恢复 pi launcher 的逻辑**必须保留**（AGENTS.md 受保护）——lefthook 接管时把这段迁进 lefthook 的 pre-commit，或继续用 core.hooksPath 并行。

**验收**：故意引入一个会失败的测试 → push 被 CI/hook 拦下。

### 步骤 ② 临时文件安全（P0，半天）

`lib.rs:2593/2605/2609` 把 PI session/media/summary 文件从共享 `/tmp` 固定前缀改为：
```rust
use rand::distributions::Alphanumeric;

fn secure_temp_path(app: &AppHandle, purpose: &str) -> PathBuf {
    // 私有目录：app_data_dir/.tmp/，权限 0700
    let dir = app.path().app_data_dir().unwrap().join(".tmp");
    let _ = fs::create_dir_all(&dir);
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)); }
    // 随机名：{purpose}-{random16}.jsonl
    let rnd: String = rand::thread_rng().sample_iter(&Alphanumeric).take(16).map(char::from).collect();
    dir.join(format!("{purpose}-{rnd}.jsonl"))
}
// 写入时 0o600
use std::os::unix::fs::OpenOptionsExt;
fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&path)?;
```
**验收测试**：写出的文件 mode 是 0o600、目录 0o700、名含随机段；并发两路不撞名。

### 步骤 ③ dispatcher catch_unwind（P1，半天）

为 bot worker / scheduler 回调包统一防护（抄防御模式 ⑤）：
```rust
fn safe_spawn<F>(name: &str, f: F)
where F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>> + Send + 'static
{
    tokio::spawn(async move {
        let res = std::panic::AssertUnwindSafe(f()).catch_unwind().await;
        if let Err(e) = res { log::error!("[{name}] callback panicked: {e:?}"); }
    });
}
```
在 wechat/lark/pi_bridge 的 `spawn` 调用点替换为 `safe_spawn`。**收益**：一个坏订阅者 panic 不再静默毒化状态。

**验收测试**：注入一个故意 panic 的回调 → 其他订阅者仍正常执行，错误被 log。

### 步骤 ④ redact 安全默认（P0，0.5 天）

- `redact_agent_record` 的 `include_secrets` 在 Rust 侧 `#[derive(Serialize)]` 默认 `#[serde(default)]`（false）
- 前端 `piClient.ts:835` 的 `includeSecrets: boolean` 改为 `includeSecrets?: boolean`（可选，缺省 false）
- UI 导出确认框默认不勾选"包含密钥"
- **验收**：不传 `includeSecrets` 调用导出 → 返回数据不含 client_secret/token/api_key/peer_shared_secret。

### 步骤 ⑤ emit 统一吞错 + log（P1，0.5 天）

把所有 `app.emit(...)` 裸调用改为 helper：
```rust
fn emit_safe(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    if let Err(e) = app.emit(event, payload) {
        log::warn!("emit {event} failed: {e}");  // 不向上传播到流式循环
    }
}
```
替换 `lib.rs:3944/3973` 等裸 emit。

### 步骤 ⑥ 800 行机械化（P0，半天）

`scripts/check-file-size.ts`（或 .mjs）：
```js
const LIMIT = 800;
const EXCLUDE = [/\/vendor\//, /\/node_modules\//, /\.test\./, /\.spec\./, /dist\//];
// 遍历 src-tauri/src/**/*.{rs} 和 src/**/*.{ts,tsx}
// 超过 LIMIT 且不在 EXCLUDE → 报错退出 1
```
加进 `package.json` `"check:size"` + CI + pre-push。

**验收**：故意把一个文件撑到 801 行 → 脚本报错。

### 步骤 ⑦ clippy 纪律（P1，半天）

`src-tauri/Cargo.toml`：
```toml
[lints.clippy]
all = "warn"
pedantic = "warn"     # 或先 "allow" 逐步开
# 对明显误报的规则单独 allow
```
配 `cargo clippy -- -D warnings` 进 CI。先用 `warn` 跑一轮看噪音量，再决定 pedantic 是否升 `deny`。

### 步骤 ⑧ 测试分层升级（P1，3-5 天）

- **Coverage gate**：装 `@vitest/coverage-v8`，`vite.config.ts` 加 `coverage:{ provider:'v8', thresholds:{ lines:70 } }`（先松后紧，ratchet 上去）；CI 跑 `vitest run --coverage`
- **真 snapshot**：把 styles 测试的 `readFileSync + toContain` 换成 `.toMatchFileSnapshot()` 或 `.toMatchSnapshot()`；加"会话重放 snapshot"锁定 transcript 输出（配合报告 01 的 session 日志）
- **real-API e2e 自 skip**：新 vitest 配置 `vitest.e2e.config.ts`，测试开头 `const KEY = process.env.NINECLAW_TEST_KEY; (KEY ? describe : describe.skip)('e2e', ...)`——无 key CI 绿，有 key 跑真 provider
- **真实入口路径**：至少一个 smoke 测试跑 `vite build` 产物（或 `cargo build --release` 的 binary）而非 tsx/即时编译
- **验证世界断言**：e2e 里重读文件、断言未触碰文件 byte-identical（`expect(fs.readFileSync(untouched)).toEqual(original)`）

### 步骤 ⑨ 文化基建（P2，持续）

- `docs/research-dsh/postmortem-template.md`：Executive summary / Timeline / Root cause / Guardrails 四段
- 把 `docs/DEV_NOTES.md` 的 7 条升级为正式 postmortem（补"为什么流程放过 + 新增哪条 guardrail"）
- `technical-notes/decisions/`（ADR）：Decision / Context / Alternatives considered / Rejected（标准结构）
- 复用现有 `HISTORY_*_GUARDRAILS.md` 的体例，新功能随 PR 补 guardrail

### 步骤 ⑩ 端到端 trace id（P2，1-2 天）

生成一个 trace id 在"IM 入站消息"处，沿 `pi spawn → LLM 调用 → 回复投递` 传递（注入 env / PI RPC payload / llm_trace entry），让"某条微信消息为何没回"可一键串起 app 日志 + llm trace + dev_trace。

---

## 4. 工作量与风险

| 步骤 | 工作量 | 风险 |
|------|--------|------|
| ① CI+门禁 | 1-2 天 | 低（CI 本身不改产品逻辑）；macOS runner 较贵可先用 ubuntu 跑前端+lint，mac 跑 cargo |
| ② 临时文件 | 0.5 天 | 低（局部 IO）；需确认 PI 对新 session 路径的读取无硬编码 `/tmp` |
| ③ dispatcher | 0.5 天 | 低 |
| ④ redact 默认 | 0.5 天 | 低 |
| ⑤ emit helper | 0.5 天 | 低 |
| ⑥ 800 行脚本 | 0.5 天 | 低（可能首次跑暴露一批超标文件，按 AGENTS.md 该拆） |
| ⑦ clippy | 0.5 天起 | 中（首次噪音多，需逐条 allow/deny 决策） |
| ⑧ 测试分层 | 3-5 天 | 中（snapshot 基线需 review 每个 diff） |
| ⑨ 文化 | 持续 | 低 |
| ⑩ trace id | 1-2 天 | 中（需贯穿多模块） |

## 5. 边界与不建议

- **不建议**照搬 dsh 的 ~60 个 verify-* 门禁——维护成本极高。挑高性价比的：链接检查、800 行、coverage、clippy、duplicate（jscpd）即可。
- **不建议**一次把 coverage 阈值设到 100%——会逼出无意义测试。从 60-70% 起步，ratchet 上去。
- CI 用 macOS runner 较贵；可分层：ubuntu runner 跑前端 lint/test/typecheck（快、便宜），macOS runner 只跑 cargo（必需，因 Tauri/macOS 优先）。
- clippy pedantic 一次性全开会产生大量噪音，先 `all=warn` 跑基线，再逐条评估 pedantic。
- 这块**完全不碰 PI**，是风险最低、收益最直接的一组抄作业。建议作为第一批落地（P0 全做）。
