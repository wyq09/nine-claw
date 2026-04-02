# NineClaw 开发注意事项

> 记录开发过程中遇到的关键问题和容易踩坑的点，避免重复犯错。

---

## 1. 微信 Bot：QR 登录后必须自动启动轮询

**日期**：2026-03-28

**问题**：`handleWechatLogin` 扫码成功后把 `status` 设为 `'已连接'`，但从未调用 `botStartWechat()`。UI 看到 `'已连接'` 就隐藏了"启动 Bot"按钮、显示"Bot 正在运行"，而后端的 monitor/worker 线程根本没启动，导致收不到任何消息。

**根因**：前端状态 (`status`) 驱动了 UI 分支渲染——`'已连接'` 会隐藏启动入口。登录和启动是两个独立操作，登录只拿到 `bot_token`，启动才会创建轮询线程。

**修复**：在 `handleWechatLogin` 登录成功后，立即用刚获取的 `loginToken` 和 `loginBaseUrl` 调用 `botStartWechat()`，而不是依赖用户手动点击。

**教训**：
- 任何将状态设为"已就绪"的操作，必须确保对应的后端服务也真正启动了。
- 不要让 UI 状态跑在实际服务状态前面。

---

## 2. React 状态批量更新：不能立即读取刚 set 的值

**场景**：`handleWechatLogin` 中先 `updateBotConfig('wechat', { token: result.bot_token })`，然后 `handleWechatStart` 里读 `botConfigs.wechat.token`——此时拿到的是旧值。

**正确做法**：用局部变量保存关键值（如 `loginToken`、`loginBaseUrl`），直接传给后续调用，不要依赖 React state 的即时更新。

```typescript
// ✅ 正确：用局部变量
const loginToken = result.bot_token ?? ''
await botStartWechat(loginToken, { ... })

// ❌ 错误：依赖刚 set 的 state
updateBotConfig('wechat', { token: result.bot_token })
await handleWechatStart() // 这里读到的 token 可能还是旧值
```

---

## 3. WeChat Channel 架构：自包含处理模式

微信通道采用**自包含 (self-contained)** 架构，不走 `ChannelManager` 的统一消息处理循环：

- **Monitor 线程**：`getUpdates` 长轮询，间隔 `POLL_INTERVAL_SECS`（3 秒）
- **Worker 线程**：通过 `PiBridge` 调用 pi 子进程处理消息，流式回调 chunk 到前端
- **回复投递**：Worker 完成后直接调 `send_reply_chunks` 发送完整回复给微信

这意味着：
- `ChannelManager.ensure_processing()` / `process_incoming_messages()` 对微信通道不生效（`_tx` 参数未使用）
- 微信的 AI 配置（provider/model/key）在 `channel.set_ai_config()` 中设置，传入 Worker 线程
- `context_token` 是 iLink 协议的关键字段，每条用户消息都会带上，回复时必须使用

---

## 4. iLink 协议关键约束

- `context_token`：每次用户消息附带，**回复时必须原样传回**，否则消息无法投递到正确的会话
- 消息分片：单条微信消息上限约 3900 字符（`CHUNK_SIZE`），超长回复需分片发送，分片时注意 Unicode 字符边界
- `errcode -14`：会话过期，需要重新登录
- `message_type == MSG_TYPE_USER`：只处理用户消息，过滤系统/bot 自身消息

---

## 5. Rust 字符串截断：中文必须按字符而非字节切片

**日期**：2026-03-28

**问题**：`&s[..s.len().min(N)]` 是按**字节**截取。中文字符每个占 3 字节，截到 N 字节时可能落在字符内部，导致 panic：
```
byte index 30 is not a char boundary; it is inside '个' (bytes 29..32)
```

**正确做法**：用 `truncate_chars` 按字符数截取：
```rust
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

// ✅ 正确
log::info!("{}", truncate_chars(&text, 50));

// ❌ 错误（中文必 panic）
log::info!("{}", &text[..text.len().min(50)]);
```

同样，分片发送长文本时也要按字符边界切，参见 `send_reply_chunks` 和 `send_message` 的实现。

---

## 6. Channel Factory 模式

新增通道时的步骤：

1. 在 `factory.rs` 的 `ChannelConfig` 枚举中添加变体
2. 实现 `Channel` trait（注意 `Send + Sync` 约束）
3. 在 `create_channel` 中添加 match arm
4. 在 `lib.rs` 中添加对应的 Tauri command
5. 前端 `piClient.ts` 中添加 invoke 封装

---

## 6. PiBridge 会话隔离

`PiBridge` 为每个 `(channel_id, user_id)` 对生成独立的 session 文件（MD5 哈希），路径格式：

```
/tmp/nineclaw-bot-session-{hash}.jsonl
```

注意：
- pi 子进程是**一次性的**（prompt → response → 退出），不是长驻进程
- stdin 写入 prompt 后立即 `drop(stdin)` 通知 pi 输入结束
- 如果 pi 未安装或不在 PATH 中，会报 `启动 pi 失败` 错误

---

## 7. 前端事件流

后端通过 Tauri `emit` 向前端推送事件：

| 事件名 | 用途 |
|--------|------|
| `pi://stream` | 主聊天流式输出（delta / done / error / tool calls） |
| `bot://message` | Bot 通道消息（inbound / outbound_chunk / outbound_done） |
| `bot://status` | Bot 运行状态日志（processing / done / warn / error） |
| `bot://qr-code` | 微信 QR 登录状态（waiting / scanned / confirmed） |

前端通过 `@tauri-apps/api/event` 的 `listen` 订阅，记得在组件卸载时 `unlisten`。
