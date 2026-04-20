/// NineClaw 定时任务元数据提炼（title / summary / goal）发送给模型的完整提示词模板。
pub fn build_task_metadata_prompt(
    task_type_label: &str,
    schedule_hint: &str,
    goal: &str,
) -> String {
    format!(
        "你是 NineClaw 定时任务的文案编辑。用户原始表述可能含闲聊、重复或口语，请提炼为三部分，写入 JSON。\n\
不要执行任何任务、不要编造用户未表达的需求、不要输出思考过程。\n\
规则：\n\
- title：4～20 个字的列表短标题，不用书名号，不要用「定时任务」开头\n\
- summary：20～100 字的一句话说明（列表「描述」列）；不要逐字复制用户原话开头；具体触发时间已在调度里单独存储，summary 不必重复钟点\n\
- goal：到点提醒或唤起智能体执行时使用的**任务正文**——简洁、可执行、用书面语重写；去掉无关闲聊与重复；保留用户真正要做的那件事；不要整段粘贴聊天记录\n\
\n\
任务类型：{task_type_label}\n\
调度（帮助理解语境）：{schedule_hint}\n\
\n\
用户原始表述：\n\
{goal}\n\
\n\
只输出一行合法 JSON，不要 markdown 代码块，格式：{{\"title\":\"...\",\"summary\":\"...\",\"goal\":\"...\"}}"
    )
}
