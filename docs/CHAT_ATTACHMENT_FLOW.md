# Chat Attachment Flow

## Supported Inputs

- 图片
- 视频
- 语音 / 音频
- 文档和普通文件
  - `.txt`
  - `.md`
  - `.pdf`
  - `.doc` / `.docx`
  - `.xls` / `.xlsx`
  - 以及其他本地文件

## Desktop Composer

桌面聊天输入框现在支持两条导入链路：

1. 点击输入框工具栏的附件按钮
2. 在输入框里直接粘贴文件、截图，或粘贴 Finder 的本地文件路径

导入后文件会先被复制到：

- `agents/<agent-id>/inbox/YYYY-MM-DD/`

然后：

- 在输入框里显示为附件 chip
- 发送时自动拼到用户消息里
- 聊天时间线中的用户气泡会渲染成可预览图片 / 视频 / 音频 / 可打开文件卡片

## Prompt Serialization

发送给 `pi` 的消息正文会在原始文本后追加 markdown 链接形式的本地路径，例如：

```md
请帮我总结这个文档。

[需求说明.pdf](/absolute/path/to/file.pdf)
```

这样做有两个目的：

1. UI 能直接复用现有 markdown / 附件渲染逻辑
2. `pi` 能直接看到绝对路径，并在需要时用工具读取这些本地文件

## Memory Integration

每个导入附件还会登记到 agent 的：

- `memory/SOURCE_INDEX.md`
- `memory/LOG.md`

这样附件不只是“发过去一次就没了”，而是变成这个 agent 的 raw source 资产。
