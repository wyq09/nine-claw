pub fn build_user_kv_memory_reorganize_prompt(
    scope_description: &str,
    markdown_pack: &str,
    conversation_digest: &str,
    existing_kv_outline: &str,
) -> String {
    format!(
        r#"你是 NineClaw「用户记忆 K/V」整理助手。只能从给定材料中提取用户侧的长期可复用事实与偏好；
不要编造未出现的事实；丢弃寒暄。

分类（category 字段必须小写）：identity（身份画像）、work（工作方式）、writing（写作输出习惯）、directive（对用户 AI 的长期指令）。
每条 text 用 1～3 句中文，不超过 380 字符，不要使用 Markdown 代码围栏。

若有「已有 K/V 摘录」且无新增语义，请勿重复条目。

只输出一行 JSON：`{{"items":[{{"category":"identity","text":"示例"}}]}}`，items 最多 26 条。
category 仅能取 identity、work、writing、directive。无可写项则输出 `{{"items":[]}}`。

## 归属范围
{scope}

## Markdown / 文稿
{markdown}

## 聊天摘录
{chat}

## 已有 K/V 摘录
{kvex}
"#,
        scope = scope_description.trim(),
        markdown = nonempty_or_placeholder(markdown_pack),
        chat = nonempty_or_placeholder(conversation_digest),
        kvex = nonempty_or_placeholder(existing_kv_outline),
    )
}

fn nonempty_or_placeholder(block: &str) -> String {
    let s = block.trim();
    if s.is_empty() {
        "_暂无_".to_string()
    } else {
        s.to_string()
    }
}
