---
name: nineclaw-atomic-tools
description: NineClaw 基础原子工具规范。统一 read_file、write_file、edit_file、bash、list_dir、grep、glob、web_search、web_fetch 九类能力的语义、输入边界、组合方式和扩展规则，供后续 skills 复用。
triggers:
  - read_file
  - write_file
  - edit_file
  - list_dir
  - grep
  - glob
  - web_search
  - web_fetch
  - bash
  - 原子工具
  - 基础工具
capabilities:
  - read_file
  - write_file
  - edit_file
  - bash
  - list_dir
  - grep
  - glob
  - web_search
  - web_fetch
sideEffectLevel: high
modes:
  - all
---

# NineClaw Atomic Tools

本技能定义所有 NineClaw skills 可复用的基础工具契约。它不是业务技能，而是底层能力规范。业务技能应组合这些原子能力完成任务，不要重新定义同义工具名。

## 命名规范

对外稳定名称使用 snake_case；当前 PI runtime 的真实工具名使用短名。调用工具时以运行时实际可用名称为准：

| 规范能力 | PI 工具 | 副作用 | 用途 |
| --- | --- | --- | --- |
| `read_file` | `read` | none | 读取单个文件内容或图片 |
| `write_file` | `write` | high | 创建或整体覆盖文件 |
| `edit_file` | `edit` | high | 对已有文件做精确文本替换 |
| `bash` | `bash` | variable | 执行 shell 命令 |
| `list_dir` | `ls` | none | 列出目录条目 |
| `grep` | `grep` | none | 在文件内容中搜索文本或正则 |
| `glob` | `find` | none | 按 glob/pattern 查找文件路径 |
| `web_search` | `web_search` | low | 搜索和发现网页、新闻、图片、视频结果 |
| `web_fetch` | `web_fetch` | low | 抓取并解析指定网页内容 |

## 通用调用原则

- 优先使用最小副作用工具：能 `read_file/list_dir/grep/glob/web_search/web_fetch` 解决时不要用 `bash`。
- 修改已有文件优先用 `edit_file`，只有新建文件或整体重写才用 `write_file`。
- 路径默认相对当前工作目录；跨目录、生成物、附件和用户给出的文件使用绝对路径。
- 需要最新外部信息时先用 `web_search` 发现候选来源，再用 `web_fetch` 读取具体页面。
- 工具输出可能截断；看到 offset、limit、truncation 或完整输出路径提示时继续读取，不要猜测缺失内容。
- 业务 skill 可以声明自己依赖哪些规范能力，但不要改变这些能力的语义。

## 原子工具契约

### `read_file`

PI 工具：`read`

输入：

```json
{ "path": "src/App.tsx", "offset": 1, "limit": 120 }
```

规则：

- 用于读取文件内容、局部片段或图片。
- 大文件必须分段读取。
- 读取后再决定是否编辑，避免基于过期上下文修改文件。

### `write_file`

PI 工具：`write`

输入：

```json
{ "path": "docs/example.md", "content": "# Example\n" }
```

规则：

- 只用于新建文件、生成完整产物、或用户明确要求整体覆盖。
- 覆盖已有手写源码前必须先读取并确认不会丢失未相关内容。
- 写入内容应是完整文件内容，不是 patch 片段。

### `edit_file`

PI 工具：`edit`

输入：

```json
{
  "path": "src/App.tsx",
  "edits": [
    { "oldText": "const oldValue = true", "newText": "const oldValue = false" }
  ]
}
```

规则：

- `oldText` 必须足够精确，避免误替换。
- 多处相关替换可以放在同一次 `edits` 中。
- 如果替换失败，先重新读取目标文件，不要扩大修改范围。

### `bash`

PI 工具：`bash`

输入：

```json
{ "command": "npm test -- --run", "timeout": 120000 }
```

规则：

- 用于构建、测试、格式化、代码生成、复杂查询和需要系统命令的操作。
- 读文件、搜文本、列目录优先使用专用原子工具。
- 高风险命令必须有明确用户意图，例如删除、移动、提交、发布、安装依赖。

### `list_dir`

PI 工具：`ls`

输入：

```json
{ "path": "src-tauri/src", "limit": 200 }
```

规则：

- 用于了解目录结构和文件名。
- 只需要路径列表时优先使用 `glob`。
- 需要文件内容时再接 `read_file`。

### `grep`

PI 工具：`grep`

输入：

```json
{
  "pattern": "select_skills_for_turn",
  "path": "src-tauri/src",
  "glob": "*.rs",
  "ignoreCase": false,
  "literal": true,
  "context": 2,
  "limit": 50
}
```

规则：

- 用于按内容查找。
- 精确字符串搜索设置 `literal: true`。
- 需要限定文件类型时使用 `glob` 字段。

### `glob`

PI 工具：`find`

输入：

```json
{ "pattern": "**/*.tsx", "path": "src", "limit": 200 }
```

规则：

- 用于按路径模式查找文件。
- 不读取文件内容。
- 结果很多时收窄 `path` 或 `pattern`。

### `web_search`

PI 工具：`web_search`

输入：

```json
{
  "query": "site:docs.rs tokio select macro",
  "region": "global",
  "engines": ["bing_int", "duckduckgo"],
  "limit": 5,
  "timeRange": "past_year",
  "searchType": "web",
  "language": "en-US"
}
```

常用字段：

- `query`：必填，普通搜索 query，也可以包含 `site:` 等高级操作符。
- `engine` / `engines`：可选，指定一个或多个搜索引擎。
- `region`：可选，`cn`、`global` 或 `all`。
- `limit`：可选，每个搜索引擎最多返回 1 到 10 条结果。
- `site`、`fileType`、`exactTerms`、`excludeTerms`、`orTerms`：可选，结构化搜索约束。
- `timeRange`：可选，`past_hour`、`past_day`、`past_week`、`past_month`、`past_year`。
- `searchType`：可选，`web`、`news`、`images`、`videos`。
- `maxResultSizeChars`：可选，限制直接返回给模型的字符数。

规则：

- 用于发现网页和候选来源，不用于读取完整网页内容。
- 搜索结果必须按来源质量筛选；需要引用或精确内容时继续调用 `web_fetch`。
- 对时效性问题优先设置 `timeRange`，并在回答中说明来源时间。
- 多来源比对时使用 `engines`，避免只依赖单个搜索结果页。

### `web_fetch`

PI 工具：`web_fetch`

输入：

```json
{
  "url": "https://example.com/article",
  "ua": "Mozilla/5.0"
}
```

规则：

- 用于读取一个明确的 `http` 或 `https` URL。
- `url` 必须是绝对 URL。
- `ua` 可选；一般留空，微信公众号等页面由 runtime 自动选择合适 UA。
- 对网页事实、引用、文章摘要和页面内容判断，必须以 `web_fetch` 内容为准，而不是只看搜索摘要。
- 如果页面是二进制、反爬拦截、编码不支持或内容过大，根据工具返回的错误或临时文件路径继续处理。

## 组合模式

常见工作流：

- 定位文件：`glob` -> `grep` -> `read_file`
- 小范围修改：`read_file` -> `edit_file` -> `bash` 测试
- 新增文件：`list_dir` -> `write_file` -> `bash` 测试
- 重构前摸底：`glob` -> `grep` -> 多个 `read_file`
- 外部研究：`web_search` -> 筛选来源 -> `web_fetch` -> 归纳结论
- 网页内容处理：用户给 URL -> `web_fetch` -> 必要时 `write_file` 保存摘要或产物

## 横向扩展规范

新增原子工具时必须满足：

1. 名称使用 snake_case，动词开头，语义单一。
2. 输入是 JSON object，字段稳定、可序列化、可审计。
3. 明确副作用级别：`none`、`low`、`medium`、`high`。
4. 明确路径解析规则、输出截断规则和错误语义。
5. 能由业务 skill 组合使用，不把多个业务步骤打包进一个原子工具。
6. 新工具加入本规范表格，并为至少一个业务 skill 提供组合示例。

## 禁止事项

- 不要创建 `readFile`、`read-file`、`file_read` 等同义名称。
- 不要让业务 skill 重新解释 `write_file` 或 `edit_file` 的覆盖语义。
- 不要用 `bash` 绕过已有原子工具，除非命令本身就是任务目标或专用工具不够表达。
- 不要用 `web_search` 代替 `web_fetch` 做网页内容阅读或事实引用。
- 不要把专门站点 API、登录态浏览器自动化、支付、数据库访问包装进 `web_fetch`；这些应是独立原子工具或上层 skill。
- 不要把鉴权、业务流程、外部 API 编排做成原子文件工具；这些应是上层 skill。
