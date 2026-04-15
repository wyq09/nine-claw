# 微信通道：出站发送文件 / 图片 / 视频（实现流程）

本文描述 NineClaw 从客户端到 iLink Bot API 的**出站媒体**链路，对应实现主要在：

- `src-tauri/src/lib.rs` — Tauri 命令 `bot_send_media`
- `src-tauri/src/channels/manager.rs` — `ChannelManager::send_media` 按 `channel_id` 分发
- `src-tauri/src/channels/wechat/mod.rs` — `WeChatChannel::send_media`
- `src-tauri/src/channels/wechat/api.rs` — `WeChatApi::send_binary_media` 及 CDN 上传

前端侧：`src/lib/piClient.ts` 的 `botSendMedia`；仅当会话带 `botTarget` 且输入框有附件时走该路径（见 `NineClawApp` 的 `handleSubmit`）。

---

## 1. 入口：`bot_send_media`

1. 从本地路径 `fs::read` 读入字节；`file_name` 取路径最后一段。
2. 将字符串 `media_type`（`image` / `video` / `audio` / `voice` / `file` 等）映射为 `MediaType`。
3. 组装 `MediaPayload { media_type, file_name, data }`，调用 `channel_manager().send_media(channel_id, user_id, &payload)`。

失败时常见：`读取文件失败`、`锁失败`、或下层返回的 HTTP / CDN 错误字符串。

---

## 2. 微信通道：`WeChatChannel::send_media`

1. 构造 `WeChatApi::new(base_url, token, route_tag)`（与文本发送一致）。
2. 按 `MediaPayload.media_type` 映射为 iLink **消息项类型**常量：
   - 图片 → `MSG_ITEM_TYPE_IMAGE`
   - 视频 → `MSG_ITEM_TYPE_VIDEO`
   - 音频与普通文件 → 均走 `MSG_ITEM_TYPE_FILE`（与 `send_media_item` 等 helper 一致）
3. 从 `context_tokens` 里按 `user_id` 取出 `context_token`（若有），传入 API。
4. 在 worker 线程里 `block_on(api.send_binary_media(...))`。

---

## 3. 核心：`WeChatApi::send_binary_media`（`api.rs`）

整体：**先上传到微信 CDN（密文），再发一条带媒体引用的 `sendmessage`**。

### 3.1 按 `media_type` 分支

| 入参 `media_type` | CDN `upload_media_type` | 最终 `item_list` 结构 |
|-------------------|-------------------------|------------------------|
| `MSG_ITEM_TYPE_IMAGE` | `UPLOAD_MEDIA_TYPE_IMAGE` (1) | `image_item`（含 `mid_size` = 密文长度） |
| `MSG_ITEM_TYPE_VIDEO` | `UPLOAD_MEDIA_TYPE_VIDEO` (2) | `video_item`（含 `video_size` = 密文长度） |
| `MSG_ITEM_TYPE_FILE` / `MSG_ITEM_TYPE_VOICE` | `UPLOAD_MEDIA_TYPE_FILE` (3) | `file_item`（含 `file_name`、`len` = **明文**长度字符串） |

不认识的 `media_type` 直接 `Err`（`不支持的微信媒体类型`）。

### 3.2 `upload_media`（每种媒体各执行一次）

1. 对**明文**算 MD5（`rawfilemd5`）。
2. 生成随机 16 字节 AES key，`aeskey` 以 hex 字符串形式参与后续请求。
3. 使用 **AES-128-ECB** 对明文加密，得到 `ciphertext`。
4. 生成 `filekey`（UUID）。
5. **`get_upload_url`**：`POST {base_url}/ilink/bot/getuploadurl`  
   Body 含 `filekey`、`media_type`、`to_user_id`、`rawsize`、`rawfilemd5`、`filesize`（密文长度）、`aeskey`、`no_need_thumb` 等。  
   响应中取得 `upload_full_url` 和/或 `upload_param`。
6. **`upload_encrypted_media_to_cdn`**：向 CDN 上传 **密文**（`Content-Type: application/octet-stream`）。
   - URL 优先用响应里的 `upload_full_url`；否则用 `upload_param` 拼固定前缀 `WECHAT_CDN_BASE_URL`（`novac2c.cdn.weixin.qq.com/c2c/upload?...`）。
   - 成功时从响应头读取 **`x-encrypted-param`**，作为后续 `encrypt_query_param`；失败时可能带 `x-error-message`。
   - 最多重试 `CDN_UPLOAD_MAX_RETRIES` 次，间隔 300ms。
7. 返回 `UploadedMediaInfo`：`encrypt_query_param`、`aeskey_hex`、明文长度、密文长度等。

### 3.3 组装消息并发送

将 CDN 返回的 `encrypt_query_param`、Base64 编码后的 `aes_key`、`encrypt_type: 1` 填入对应 `image_item` / `video_item` / `file_item` 的 `media` 字段，然后调用 **`send_media_message`**。

**`send_media_message`**：`POST {base_url}/ilink/bot/sendmessage`  
Body 结构与文本类似：`msg.to_user_id`、`client_id`、`message_type`、`message_state`、`item_list`（单元素，类型与 JSON 与上面一致）、`context_token`、`base_info.channel_version`。

日志：`微信上传媒体完成: to_user_id=... upload_media_type=... item_type=... file_name=...`

---

## 4. 超时与常量（`api.rs` 顶部）

- `API_TIMEOUT_MS`：`getuploadurl`、非 CDN 的 API 请求。
- `CDN_UPLOAD_TIMEOUT_MS`：CDN POST 上传。
- `GET_UPDATES_TIMEOUT_MS`：仅用于 `get_updates` 长轮询，与发送媒体无关。

HTTP 客户端：若本机 `http(s)_proxy` / `all_proxy` 不可用则 `no_proxy()`，避免错误走代理导致连不上。

---

## 5. 与「入站收附件」的区别

入站下载、解密 CDN、落盘等逻辑在 `wechat/mod.rs`（如 `persist_wechat_attachment`、`download_cdn_bytes`），**不是** `send_binary_media` 这条出站链。本文仅覆盖**用户从 NineClaw 发往微信用户/机器人**的媒体发送。

---

## 6. 排查问题时建议对照的错误前缀

| 字符串片段 | 大致阶段 |
|------------|----------|
| `读取文件失败` | `bot_send_media` 读盘 |
| `getUploadUrl HTTP` / `getUploadUrl 请求失败` | 申请上传地址 |
| `微信 CDN 上传` | POST 密文到 CDN |
| `sendMediaMessage HTTP` | `ilink/bot/sendmessage` |

将完整错误（含 HTTP body 片段）与上述阶段对照即可快速定位。
