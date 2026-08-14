# Postmortem-0002: React 状态批量更新误读旧值

- 来源：`docs/DEV_NOTES.md` #2

## 1. 执行摘要

`handleWechatLogin` 先 `updateBotConfig('wechat', { token })`，随后 `handleWechatStart` 读 `botConfigs.wechat.token` 时拿到的是旧值，导致后续启动使用过期 token。修复为用局部变量保存关键值并直接传递。

## 2. 时间线

- 场景：登录后 set token → 紧随其后读同一 state → 读到旧值。
- 定位：React setState 是异步批量更新，同一执行上下文内读取不保证已更新。
- 修复：`loginToken` / `loginBaseUrl` 用局部变量保存，直接传给后续调用。

## 3. 根因

- 直接原因：在 `setState` 后同步读取同一 state 字段。
- 系统性原因：把"异步批量更新的 state"误当成"同步可读的变量"，依赖其即时更新。

## 4. Guardrails

- 新增护栏（教训）：跨调用传递关键值一律用局部变量，不依赖刚 set 的 state。
- 为什么流程放过了它：无 lint/类型约束能静态发现"set 后同 tick 读"，且旧值不报错、只是悄悄用错。
- 新检查：评审中识别 `updateX(...)` 后立即读 `state.x` 的模式；必要时把"set 后取值的 key"抽象为返回局部值的 helper，从根上消除该模式。
