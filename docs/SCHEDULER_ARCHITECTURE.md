# NineClaw Scheduler Architecture

## Goal

把当前嵌在 GUI 进程内的 `heartbeat` 扫描线程，升级成一个可常驻、可恢复、可跨平台托管的 scheduler 子系统。

目标平台先覆盖：

- macOS
- Windows

目标能力：

- 支持 one-shot、interval、cron-like schedule
- 支持 shell / agent / notify 三类任务
- 支持应用关闭后仍继续执行
- 支持失败重试、补跑、去重、审计
- 保持一套统一调度语义，不把业务逻辑散落到各平台原生调度器里

非目标：

- 第一版不追求集群调度
- 第一版不做系统级 `root` / `LocalSystem` 权限运行
- 第一版不把每个 job 映射成独立 OS-level cron / Task Scheduler task

## Current State

当前仓库已经有一个 scheduler 雏形，但能力比较局限：

- 应用启动时在 [`src-tauri/src/lib.rs`](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:3070) 直接调用 `start_heartbeat_scheduler`
- `heartbeat` 在 [`src-tauri/src/heartbeat.rs`](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/heartbeat.rs:72) 内起一个后台线程，每 30 秒扫描一次
- 任务定义挂在 agent 的 `heartbeat_config` 上，结构见 [`src-tauri/src/agents.rs`](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agents.rs:93)
- 运行去重依赖 SQLite `agent_heartbeat_runs` 表上的唯一键，见 [`src-tauri/src/heartbeat.rs`](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/heartbeat.rs:47)

现状问题：

1. GUI 退出后调度就消失
2. 调度表达式过弱，目前基本只有 `daily + HH:MM`
3. `timezone` 还是偏字符串/固定偏移处理，不适合跨地区和 DST
4. 调度扫描、任务执行、消息投递都耦合在一个模块里
5. 还没有真正的 daemon install / uninstall / health / recovery 机制

## Design Principles

### 1. 平台调度器只负责“保活”，不负责业务语义

macOS `LaunchAgent` 和 Windows `Task Scheduler` 只负责让 `nineclaw-scheduler` 常驻或自动恢复。

真正的 job 解析、错过执行、补跑、重试、并发控制、审计记录，全部由 NineClaw 自己的 scheduler core 统一实现。

这样可以保证：

- macOS / Windows 的行为一致
- UI、CLI、daemon 共享一套逻辑
- 后续加 Linux 不需要重写业务层

### 2. 调度定义与执行记录分离

任务定义是 control plane，运行记录是 data plane。

- job definition：用户想跑什么、何时跑、送到哪里
- run record：某次实际执行何时被 claim、何时开始、何时结束、结果如何

### 3. Fresh session execution

参考 Hermes cron，每次 scheduled run 都应该是 fresh execution context。

不要复用 GUI 会话状态。否则会出现：

- 历史上下文污染
- 多 job 互相串台
- daemon 重启后难以恢复

对 agent 任务，fresh session 仍然可以注入：

- agent config
- workspace root / agent home
- attached skills
- run metadata

### 4. SQLite 仍作为单机真相源

NineClaw 当前已经以 SQLite 为主存储，第一版 scheduler 不应引入额外服务。

建议继续用同一个 `nineclaw.sqlite3`，但新增 scheduler 专属表。

## Proposed Process Model

建议拆成三个层次。

### A. GUI app: `NineClaw`

职责：

- 编辑 job
- 展示 job / run 历史
- 安装或卸载后台 scheduler
- 手动触发 run、暂停、恢复
- 显示 daemon 健康状态

不负责：

- 长期扫描 due jobs
- 长时间执行任务

### B. Scheduler daemon: `nineclaw-scheduler`

这是新增的常驻进程，推荐作为第二个 Rust bin。

职责：

- 加载 scheduler config
- 维护单实例 leader lock
- 周期扫描 due jobs
- claim run
- 分发到本地 worker
- 更新 run 状态
- 进行 retry / catch-up / misfire 决策
- 写审计日志

### C. Optional control CLI: `nineclawctl`

不是必须，但非常有价值。

职责：

- `scheduler install`
- `scheduler uninstall`
- `scheduler status`
- `scheduler run <job-id>`
- `scheduler logs`
- `scheduler doctor`

这样 GUI 和终端都能操作 scheduler，不会把所有运维入口绑死在前端里。

## Recommended Module Layout

建议不要把所有逻辑继续堆在 `heartbeat.rs`。

推荐拆成：

```text
src-tauri/src/
  scheduler/
    mod.rs
    model.rs
    storage.rs
    planner.rs
    claim.rs
    executor.rs
    delivery.rs
    daemon.rs
    service_install.rs
    policy.rs
```

职责建议：

- `model.rs`: job、trigger、run、retry policy、delivery target
- `storage.rs`: SQLite schema 与 CRUD
- `planner.rs`: `next_run_at` 计算、cron / interval 解析、catch-up
- `claim.rs`: leader lock、run claim、lease renew、stale recovery
- `executor.rs`: shell / notify / agent 三类执行器
- `delivery.rs`: IM、local output、future webhook/email 等
- `daemon.rs`: 主循环
- `service_install.rs`: macOS / Windows 安装卸载
- `policy.rs`: 并发、超时、重试、misfire 策略

`heartbeat.rs` 可以先变成 scheduler 的兼容 facade，后续再删。

## Data Model

建议把现有 `heartbeat_config` 逐步迁移成更通用的 scheduler 实体。

### 1. `scheduled_jobs`

建议字段：

- `id`
- `kind`
  - `shell`
  - `notify`
  - `agent`
- `name`
- `description`
- `enabled`
- `owner_agent_id`
- `trigger_type`
  - `once`
  - `interval`
  - `cron`
- `trigger_spec_json`
- `timezone`
- `payload_json`
- `delivery_json`
- `retry_policy_json`
- `concurrency_policy`
  - `forbid`
  - `queue`
  - `replace`
- `misfire_policy`
  - `skip`
  - `run_once`
  - `catch_up`
- `next_run_at`
- `last_run_at`
- `created_at`
- `updated_at`

### 2. `scheduled_job_runs`

建议字段：

- `id`
- `job_id`
- `scheduled_for`
- `claimed_at`
- `started_at`
- `finished_at`
- `status`
  - `queued`
  - `running`
  - `success`
  - `failed`
  - `canceled`
  - `skipped`
- `attempt`
- `worker_id`
- `summary`
- `details`
- `error`
- `output_path`

唯一键建议：

- `UNIQUE(job_id, scheduled_for, attempt)`

如果第一版不做多次 retry record，也可以先用：

- `UNIQUE(job_id, scheduled_for)`

### 3. `scheduler_leases`

用于 daemon 单实例与 run lease。

建议字段：

- `lease_key`
- `owner_id`
- `leased_until`
- `updated_at`

最少需要两类 lease：

- `daemon_leader`
- `run:<run-id>`

## Trigger Model

建议统一成三种 trigger。

### Once

示例：

- `2026-04-08T18:30:00+08:00`

用途：

- 单次提醒
- 延迟任务

### Interval

示例：

- every 15m
- every 2h

语义：

- 基于上次计划触发点计算下一次，不基于“任务结束时间”漂移

### Cron

示例：

- `0 9 * * 1-5`
- `*/10 * * * *`

建议：

- 使用标准 5-field cron
- 时区使用 IANA timezone，例如 `Asia/Shanghai`、`America/Los_Angeles`
- 不再只靠固定 offset

Rust 侧建议补：

- `chrono-tz`
- 稳定 cron parser crate

## Scheduler Loop

不要固定“永远 30 秒扫描一次”作为唯一模型。

建议主循环：

1. 启动时尝试获取 `daemon_leader` lease
2. 读取最近一批 `next_run_at <= now + lookahead`
3. 计算最近一次唤醒时间
4. `sleep(min(next_due_in, max_tick_interval))`
5. 到点后事务内 claim due runs
6. 分发执行
7. 更新 `next_run_at`
8. 续租 leader lease

参数建议：

- `max_tick_interval`: 30s
- `lookahead`: 60s
- `lease_ttl`: 90s

这样兼顾：

- 空闲时不高频空转
- 机器休眠恢复后也能很快 catch up

## Claim and Concurrency

这是核心，不能只靠“线程内别重复扫到”。

建议每次执行前都走原子 claim：

1. 事务内读取 due job
2. 根据 `concurrency_policy` 判断是否允许新 run
3. 插入 `scheduled_job_runs`
4. 写入 `run:<run-id>` lease
5. 提交事务

并发策略建议：

- `forbid`: 该 job 有 running run 时，不再新开
- `queue`: 当前 run 未结束时，把本次计划点留待后续补跑
- `replace`: 取消旧 run，启动新 run

默认建议用 `forbid`。

此外再加一个全局并发上限：

- `max_parallel_runs`

建议第一版默认：

- `2` 或 `4`

避免 agent 任务和 shell 任务一起把机器打满。

## Execution Model

### Shell job

基本复用当前 `heartbeat.rs` 的 shell 执行链路，但要补三点：

1. 把执行器从 heartbeat 模块抽出去
2. 输出保存到 run artifact，而不是只截断写库
3. 增加 cancel / timeout / stale process cleanup

运行环境继续注入：

- `NINECLAW_AGENT_ID`
- `NINECLAW_AGENT_HOME`
- `NINECLAW_WORKSPACE_ROOT`
- `NINECLAW_SCHEDULED_FOR`
- `NINECLAW_RUN_ID`

### Notify job

这是最轻的一类，可直接生成文本并投递。

### Agent job

这是你这个项目最关键的一类。

建议不要让 daemon 直接复用 GUI 的 streaming command，而是抽一个“无 UI 的 agent executor”供 daemon 调用。

输入：

- agent id
- provider/model override
- prompt
- attached skills
- workspace context
- delivery target

执行原则：

- fresh session
- 禁止在 scheduled run 中递归创建 scheduler job
- 保持与普通 agent 运行一致的 runtime resolution

这部分可以复用现有 `PiBridge` / `pi_runtime` 能力，但需要把“聊天流式 UI 专用逻辑”和“后台执行逻辑”分开。

## Delivery Model

建议把“任务执行”和“投递”解耦。

delivery target 可统一为：

- `origin-im`
- `channel-user`
- `local-file`
- `none`

示例：

```json
{
  "type": "channel-user",
  "channelId": "wechat:agent-main",
  "userId": "filehelper"
}
```

这样 job 本身不需要知道 UI 在不在线。

run 完成后：

1. executor 返回 structured output
2. delivery layer 决定是否投递
3. 投递失败不覆盖 execution result，但会单独记录 delivery error

## Platform Hosting

关键建议：OS 原生能力只用来“启动 `nineclaw-scheduler`”。

### macOS

推荐 `LaunchAgent`，而不是 `LaunchDaemon`。

原因：

- 任务大概率需要用户态凭证、用户目录、聊天账号、工作区
- 不需要 root
- 更符合桌面应用场景

建议安装到：

```text
~/Library/LaunchAgents/com.wuyq.nineclaw.scheduler.plist
```

建议配置：

- `RunAtLoad = true`
- `KeepAlive = true`
- `ProcessType = Background`
- `StandardOutPath`
- `StandardErrorPath`

进程命令建议：

```bash
<NineClaw bundle helper path>/nineclaw-scheduler daemon
```

### Windows

第一选择建议是 `Task Scheduler`，不是 Windows Service。

原因：

- 用户态任务更容易拿到当前用户环境与凭证
- 安装复杂度低于 Service
- 更适合桌面应用升级与卸载
- 不需要管理员权限即可做按用户登录自启

建议任务：

- Trigger: `At log on`
- Trigger: `On startup` 可选
- Setting: failure 后自动重启
- Setting: “If task is already running” 选择不重复启动

执行命令：

```powershell
NineClaw.exe --scheduler-daemon
```

或者更清晰地打包成独立 bin：

```powershell
nineclaw-scheduler.exe daemon
```

后续如果确实有“用户未登录也要执行”的需求，再加 Windows Service 方案，但不建议第一版直接上。

## IPC and Control Plane

不建议先做复杂的本地 HTTP server。

第一版更稳的方式：

- GUI 直接写 SQLite
- daemon 轮询数据库
- GUI 通过 CLI 子命令做 install/uninstall/status

比如：

- GUI 调 `tauri::Command` 执行 `nineclawctl scheduler install`
- GUI 读数据库展示 job / run
- daemon 每个 tick 感知到变更

如果后面需要更实时的状态，再补一个本地 control socket。

## Failure Handling

必须明确以下场景。

### App 关闭

不影响 scheduler daemon。

### Daemon 崩溃

由 LaunchAgent / Task Scheduler 拉起。

### 机器休眠

恢复后 daemon 按 `misfire_policy` 处理错过的计划点。

### 执行进程卡死

run lease 过期 + 超时 kill + 标记 `failed`

### 重复执行

通过 `UNIQUE(job_id, scheduled_for)` + claim transaction 避免。

## Migration from Heartbeat

建议分阶段，不要一次性替换全部。

### Phase 1

保留现有 heartbeat UI 和数据结构，但底层新增 scheduler core。

做一个适配器：

- 读取 `heartbeat_config`
- 转成内存中的 scheduler jobs

这一步先不改 UI。

### Phase 2

新增数据库表：

- `scheduled_jobs`
- `scheduled_job_runs`
- `scheduler_leases`

heartbeat 新建/更新时同步写入 `scheduled_jobs`。

### Phase 3

引入 `nineclaw-scheduler` 常驻进程，并让现有 heartbeat 执行迁到 daemon。

此时 GUI 内的 `start_heartbeat_scheduler()` 可以只保留 dev fallback：

- daemon 不存在时，GUI 内嵌 best-effort scheduler
- 正式环境优先 daemon

### Phase 4

UI 上把“Heartbeat”正式升级为“Scheduled Tasks”。

新增：

- cron 表达式
- interval
- retry policy
- pause/resume/run-now
- run history

### Phase 5

完全移除旧 `heartbeat_config` 作为主存储，只保留兼容迁移。

## Recommended First Implementation Slice

如果要控制复杂度，我建议第一刀只做这些：

1. 新建 `scheduled_jobs` / `scheduled_job_runs` / `scheduler_leases`
2. 新增 `nineclaw-scheduler` bin
3. 把现有 `heartbeat` shell / notify 执行器抽到 scheduler executor
4. 支持：
   - daily time
   - interval
   - pause/resume
   - run-now
5. macOS `LaunchAgent` 安装
6. Windows `Task Scheduler` 安装
7. GUI 增加 daemon status 与 install button

先不要第一版就上：

- 复杂 IPC
- 多 worker 进程池
- webhook / email / SMS 等额外交付面
- 过于复杂的可视化编排

## Key Tradeoffs

### Why not use OS scheduler per job

不建议把每个 job 都变成一个 LaunchAgent / Windows Scheduled Task。

缺点：

- job 管理分散到 OS 层
- 跨平台行为不一致
- 变更、暂停、补跑、审计很难统一
- agent 任务需要大量 NineClaw 私有上下文，不适合直接裸命令拼接

### Why a dedicated daemon is worth it

相比 GUI 内线程，daemon 的优势非常明确：

- 应用关闭后仍可执行
- 有独立崩溃恢复
- 更适合长任务和重试
- 更容易做 health/status/doctor

## Suggested Naming

为了避免后续“heartbeat”和“scheduler”长期双轨混乱，建议命名尽快统一：

- 用户侧：`Scheduled Tasks`
- 内部模块：`scheduler`
- `heartbeat` 只保留兼容层名称

## Summary

最稳的方案不是“把现有 heartbeat 线程做大”，也不是“直接依赖各平台原生 cron”。

推荐方案是：

1. 用 SQLite 做统一 job / run 真相源
2. 新增 `nineclaw-scheduler` 常驻进程做统一调度
3. macOS 用 `LaunchAgent`，Windows 用 `Task Scheduler` 保活
4. GUI 退回 control plane，daemon 接管 execution plane
5. 现有 heartbeat 先适配迁移，再逐步升级为完整 scheduled tasks

这条路径和当前仓库边界最贴合，改动可控，也方便后续继续长成 Hermes 风格的 cron 子系统。
