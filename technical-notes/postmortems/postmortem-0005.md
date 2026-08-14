# Postmortem-0005: Rust 字符串按字节截断导致中文 panic

- 日期：2026-03-28
- 来源：`docs/DEV_NOTES.md` #5

## 1. 执行摘要

`&s[..s.len().min(N)]` 按字节截断，中文字符每字 3 字节，截到字符内部触发 panic（`byte index ... is not a char boundary`）。修复为 `truncate_chars` 按字符截断，分片发送同样按字符边界切。

## 2. 时间线

- 发现：日志截断中文文本时 panic，报 `not a char boundary; it is inside '个'`。
- 定位：按字节切片可能落在多字节字符内部。
- 修复：`truncate_chars` 用 `char_indices().nth(max_chars)` 找边界；`send_reply_chunks` / `send_message` 同样按字符边界切。

## 3. 根因

Rust 字符串是 UTF-8 字节序列，按字节索引切片不保证落在字符边界；中文（3 字节/字符）尤其容易在截断点触发 panic。

## 4. Guardrails

- 新增护栏：任何字符串截断/分片必须按字符边界进行，禁止 `&s[..len.min(N)]` 这类裸字节切片。
- 为什么流程放过了它：该 panic 只在特定输入（多字节字符恰好落在截断点）触发，英文文本测试覆盖不到。
- 新检查：截断/分片测试必须覆盖含多字节字符的输入；评审中禁止对用户可见文本做裸字节切片。
