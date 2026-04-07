# 虾 / 外部互通（多智能体 HTTP）

NineClaw 在**同一端口**上服务所有智能体：由请求体里的 **`toAgentId`** 指定目标智能体。  
**每个智能体有独立的入站密钥**（保存在该智能体的 `peer` 机器人绑定里），**没有**全平台共用的 Bearer Secret。

## 一句话

设置 `NINECLAW_PEER_BIND` 开启监听；在 NineClaw 里打开目标智能体 → 机器人 → **虾/对等**，复制其 **智能体 ID** 与 **入站密钥** 给对方；对方 POST 时 `toAgentId` 填该 ID，`Authorization: Bearer` 填该密钥。

## 监听端口（推荐）

在 **设置 → 通用 → 对等 HTTP（虾）** 中配置：**默认监听 `0.0.0.0:1052`**（所有网卡，局域网内其它机器可连）；可改主机/端口/是否启用；点 **「保存并重启监听」** 后立即重绑。若曾保存为 `127.0.0.1`，仅本机能访问，需改为 `0.0.0.0` 才能给**别的机器上的虾**对接。跨机时请在「对外展示基址」填写 `http://<本机局域网IP>:端口`，并放行系统防火墙对应端口。

## 环境变量（可选，优先级更高）

| 变量 | 说明 |
|------|------|
| `NINECLAW_PEER_BIND` | 若设置，则**覆盖**应用内端口配置。可写完整地址如 `0.0.0.0:1052`；仅写端口或 `:1052` 时按 **0.0.0.0:端口** 解析（可被局域网访问）。显式 `127.0.0.1:端口` 则仅本机。 |
| `NINECLAW_PEER_PUBLIC_BASE` | 可选。展示用 API 基址；也可在设置里填「对外展示基址」。未填则按监听地址推导。 |

在 **智能体编辑** 顶部 **「对等 HTTP（虾）对接」** 卡片会显示当前解析出的地址、入站 URL，以及含 **本智能体 ID 与入站密钥** 的可复制说明。

## 密钥从哪来

- **新建 / 保存智能体**时，后端会自动补全 `peer` 绑定并生成密钥（若尚未配置）。
- 应用启动时会 **backfill** 历史智能体缺失的密钥（幂等）。
- 界面可 **重新生成密钥**（立即写库）；若你清空密钥再保存，会 **保留库中旧密钥**（防误删）；要换新请用「重新生成」。

## HTTP

- **健康检查**：`GET /health`（无需鉴权），例如 `http://127.0.0.1:1052/health`；兼容别名 `GET /nineclaw/v1/health`
- **入站**：`POST /nineclaw/v1/inbound`  
  - Header：`Authorization: Bearer <该 toAgentId 对应智能体的密钥>`  
  - Body：`application/json`（字段 **camelCase**）

### 请求体字段

| 字段 | 必填 | 说明 |
|------|------|------|
| `protocol` | 是 | 固定 `nineclaw-peer` |
| `version` | 是 | 固定 `1` |
| `fromAgentId` | 是 | 对方智能体稳定 ID |
| `toAgentId` | 是 | NineClaw 内目标智能体 ID |
| `threadId` | 是 | 会话 ID，同线程多轮复用 |
| `text` | 是 | 文本内容 |
| `messageId` | 否 | 可选 |
| `replyWebhook` | 否 | 若填写：返回 **202**，完成后 POST 回复到该 URL |
| `replyWebhookAuth` | 否 | 回调时可选 `Authorization: Bearer` |
| `metadata` | 否 | 预留 |

### 统一响应信封 `PeerV1Response`（camelCase）

所有 JSON 响应均含：

| 字段 | 说明 |
|------|------|
| `protocol` | 固定 `nineclaw-peer` |
| `version` | 固定 `1` |
| `ok` | 是否成功（注意：HTTP 4xx/5xx 时 `ok` 为 `false`） |
| `kind` | 见下表 |
| `fromAgentId` / `toAgentId` / `threadId` / `inReplyTo` | 与请求 echo 对齐（HTTP 级错误在能解析请求体时也会尽量带上） |
| `reply` | 同步成功或 webhook 成功时的助手全文 |
| `text` | 仅 **`kind: inboundWebhook`** 且成功时出现，与 `reply` 相同（兼容只读 `text` 的旧对接） |
| `error` | 失败时为对象 `{ "code": "...", "message": "..." }`，成功时为省略 |
| `mode` | `inboundAccepted` 时为 `"async"` |
| `message` | `inboundAccepted` 时的人类可读说明 |
| `service` / `channelId` | 仅 **`kind: health`** |

**`kind` 取值**

| kind | 典型 HTTP | 含义 |
|------|-----------|------|
| `health` | 200 | 健康检查 |
| `inboundReply` | 200 | 同步入站结果（成功或业务失败均可能 200） |
| `inboundAccepted` | 202 | 已入队，将向 `replyWebhook` POST 结果 |
| `inboundWebhook` | （对方 URL 的响应） | 异步回调体；`fromAgentId` 为 NineClaw 侧智能体，`toAgentId` 为调用方 |
| `peerError` | 401 / 404 / 500 | 鉴权、不存在、内部错误等 |

**常见 `error.code`（处理失败 / HTTP 错误）**

| code | 说明 |
|------|------|
| `PEER_AUTH_FAILED` | 401，Bearer 与目标智能体密钥不一致 |
| `PEER_AGENT_NOT_FOUND` | 404，`toAgentId` 不存在 |
| `PEER_INTERNAL` | 500 或内部任务失败 |
| `PEER_UNSUPPORTED` | 协议 / 版本不匹配 |
| `PEER_BAD_REQUEST` | 必填字段或约束不满足 |
| `PEER_AGENT_UNAVAILABLE` | 智能体配置不可用 |
| `PEER_ABORTED` | 处理被中断 |
| `PEER_PROCESS_FAILED` | 模型/上游等其它处理错误（详见 `message`） |

### 同步（未设置 `replyWebhook`）

- 成功：**200**，`kind: inboundReply`，`ok: true`，`reply` 为助手输出。
- 业务失败仍可能 **200**，`ok: false`，`error: { code, message }`。

### 异步（`replyWebhook`）

- **202**，`kind: inboundAccepted`，`mode: "async"`。
- 完成后 **POST** 到 `replyWebhook`：`kind: inboundWebhook`，成功时 `ok: true` 且 `reply` 与 `text` 均有内容；失败时 `ok: false` 且 `error` 为结构化对象。

---

## 给对方智能体「一键复制」

以下可复制到对方项目。

---

**对接 NineClaw（每智能体不同密钥）**

1. 向用户索要：监听基址（如 `http://127.0.0.1:17312`）、**目标智能体 ID**、**该智能体的入站密钥**（三者一一对应）。
2. `POST {BASE}/nineclaw/v1/inbound`，Header：`Authorization: Bearer {该智能体的密钥}`。

```json
{
  "protocol": "nineclaw-peer",
  "version": 1,
  "fromAgentId": "你的智能体ID",
  "toAgentId": "对方提供的NineClaw智能体ID",
  "threadId": "同一会话固定ID",
  "text": "你好"
}
```

3. 与**另一个** NineClaw 智能体对话时，必须换用 **另一组** `toAgentId` + Bearer 密钥。

**curl 示例**

```bash
export NC_URL=http://127.0.0.1:17312
export TO_AGENT=某个智能体id
export AGENT_SECRET=该智能体在NineClaw里显示的入站密钥

curl -sS -X POST "$NC_URL/nineclaw/v1/inbound" \
  -H "Authorization: Bearer $AGENT_SECRET" \
  -H "Content-Type: application/json" \
  -d "{\"protocol\":\"nineclaw-peer\",\"version\":1,\"fromAgentId\":\"peer-a\",\"toAgentId\":\"$TO_AGENT\",\"threadId\":\"room-1\",\"text\":\"你好\"}"
```

---

## 实现位置

- 网关：`src-tauri/src/peer_gateway.rs`
- 密钥逻辑 / 补全 / 轮换：`src-tauri/src/agents.rs`（`apply_peer_inbound_defaults`、`backfill_peer_inbound_secrets`、`rotate_agent_peer_inbound_secret`）
- 命令：`rotate_agent_peer_inbound_secret`（前端「重新生成密钥」）
