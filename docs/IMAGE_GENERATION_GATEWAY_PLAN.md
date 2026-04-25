# 图片生成网关与 Tool 方案

## 目标

在 NineClaw 内提供一个统一的图片生成出口：

- Agent 只调用统一的 `image_generate` Tool
- Tool 不再从 Agent 配置读取模型
- Tool 直接读取系统级默认生图配置
- 后端负责把不同平台的生图接口适配成统一网关

## 当前实现边界

本次实现先落第一版基础设施：

- 系统级生图 Provider 配置
- 系统级默认生图参数配置
- 本地统一图片生成网关 `/image/:token/generate`
- PI runtime 内置 `image_generate` Tool
- Tool 自动保存生成结果到工作目录 `.nineclaw-generated-images/`

当前已支持的适配器：

- `openai_images`
- `openai_compatible`

两者当前都走 `/images/generations` 风格接口，只是语义上区分“OpenAI 原生”和“OpenAI 兼容图片网关”。

## 为什么和聊天大模型配置分开

不要把图片模型和文本大模型混在同一套配置里，原因有三类：

1. 协议不同
- 文本模型主要是 `chat/completions` / `responses` / `messages`
- 图片模型常见是 `images/generations` 或异步任务接口

2. 参数不同
- 图片模型关注 `size`、`background`、`output_format`、`quality`、`count`
- 文本模型关注 `max_tokens`、`thinking`、`context window`

3. 风险边界不同
- 生图常常有单独的账号、单独的预算、单独的速率限制
- 单独配置更容易切换供应商，也更容易审计

## 配置模型

### 1. 系统级图片 Provider 配置

存储键：

- `image_provider_configs_v1`

每个 Provider 保存：

- `adapterType`
- `baseUrl`
- `apiKey`
- `model`
- `displayName`
- `note`
- `status`

说明：

- 系统允许同时维护多个图片 Provider
- 预置 Provider 和用户新增的自定义图片 Provider 都落在同一份配置里
- Tool 执行时不会遍历全部 Provider，只会读取系统默认指向的那一个
- 设置页中的图片 Provider 编辑采用草稿模式，点击“保存图片配置”后才写入持久化存储
- 新增 `APIMart GPT-Image-2` 预置适配器，走异步任务提交 + `task_id` 轮询

### 2. 系统级默认生图配置

存储键：

- `image_generation_system_v1`

保存：

- `defaultProviderId`
- `size`
- `resolution`
- `background`
- `outputFormat`
- `quality`
- `count`

## Tool 调用链

### Agent 侧

Agent 只需要调用：

```text
image_generate(prompt, size?, resolution?, background?, outputFormat?, quality?, moderation?, outputCompression?, count?, negativePrompt?, seed?, imageUrls?, maskUrl?)
```

不需要传：

- providerId
- model
- apiKey

### Runtime Tool 侧

`image_generate` Tool 会：

1. 从运行时环境读取本地代理地址和 session token
2. 调用 `/image/:token/generate`
3. 自动保存图片到 `.nineclaw-generated-images/`
4. 把图片以内联内容返回给 Agent
5. 若是 APIMart 异步模型，由后端代理内部完成任务轮询，Tool 侧不暴露 `task_id` 轮询细节

### 后端网关侧

后端会：

1. 从当前桌面会话解析系统默认图片 Provider
2. 把图片运行时绑定进 managed runtime proxy session
3. 根据 `adapterType` 分发到不同供应商适配器
4. 统一返回：
   - `providerId`
   - `adapterType`
   - `model`
   - `taskId`
   - `revisedPrompt`
   - `images[]`

## Key 读取原则

图片模型 Key 的读取原则如下：

- Key 只保存在系统设置
- Agent 不保存图片模型 Key
- Tool 不直接读数据库
- Tool 不直接读环境变量中的真实第三方 Key
- Tool 只调用 NineClaw 本地代理
- 本地代理再读取系统设置并发起第三方请求

这样做的好处：

- Agent prompt 不会暴露密钥
- Tool 逻辑稳定，不和具体供应商耦合
- 后续更换供应商时不需要改 Agent

## 当前限制

当前版本限制：

- 只支持系统级默认生图源
- 暂不支持按 Agent 单独覆盖图片模型
- 图生图 / mask 已在协议层预留字段，但当前主要围绕 URL 型输入
- APIMart 异步任务采用请求内轮询，还没有独立后台任务队列

## 后续扩展建议

后续如果要继续扩：

1. 增加更多适配器
- `volcengine_seedream`
- `replicate_prediction`
- `fal_queue`
- `google_imagen`

2. 增加系统级 profile
- 例如 `poster-fast`、`poster-hq`、`transparent-product`

3. 再给 Agent 增加可选覆盖层
- `default_image_profile_id`

建议顺序：

- 先扩系统 profile
- 再做 Agent 覆盖

不要反过来做，否则很快会出现“每个 Agent 都单独堆一套图片配置”的维护问题。
