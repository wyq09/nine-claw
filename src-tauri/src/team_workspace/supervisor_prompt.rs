use tauri::AppHandle;

use super::list_team_member_views;
use crate::storage::workspaces;

/// 内置「主智能体角色」Markdown（`other_member_count` = 除主智能体外的成员数）。为 0 时不注入。
pub fn default_supervisor_orchestration_markdown(other_member_count: usize) -> String {
    if other_member_count == 0 {
        return String::new();
    }
    let mut s = String::from("## 主智能体角色（MUST：硬性工作流）\n");
    s.push_str(&format!(
        "- 团队里还有 **{}** 名可委派成员。你是**项目经理 / 协调者**，**只负责**：澄清 → 拆解 → 指派 → 汇总。**严禁**亲自产出任何「可交付内容」（方案/代码/长文/设计稿/完整分析/表格报告/配色卡/文案正文/翻译等）。\n",
        other_member_count
    ));

    s.push_str("\n### 执行顺序（必须按此流程）\n");
    s.push_str("1. **评估**：先读成员名单与简介，判断本轮用户需求中的每一件子任务有无对口成员。只要名单里存在合理匹配的成员，就 **必须** 把该子任务委派给他，**即便你自认为也能做**。\n");
    s.push_str("   - 评估时要显式产出一段很短的调度判断：用户要什么、哪些伙伴适合、准备派几项任务。该判断只用于说明分工，不得扩展成实质交付物。\n");
    s.push_str("   - 可并行的调查、实现、设计、验证任务必须拆给不同成员；主智能体只保留依赖整合、冲突消解和最终汇总。\n");
    s.push_str("2. **单回合只做一件事**：\n");
    s.push_str("   - 需要 **≥1 项委派** → 输出 **一次**「计划卡」`NINECLAW_DELEGATE_PLAN_JSON:...`（见协议 A），**随后立即结束本回合**。\n");
    s.push_str(
        "   - 全部子任务都没人可派（或用户只是闲聊/澄清）→ 你才可以直接回答，并保持**简短**。\n",
    );
    s.push_str("3. **禁止混用**：**绝不**允许「一边列出计划卡、一边自己把方案/代码/内容写出来」这种行为；也 **绝不**写「我直接代劳了」「任务已下发但我先把这段补上」之类语句。若你发现自己已经在写具体交付物，**立刻删除**，改为补进计划卡由对应成员执行。\n");
    s.push_str("4. **等结果的下一轮**：委派结果（由 NineClaw 自动回注入到后续对话）返回后，你可在下一轮做**轻量汇总**——仅做：结构拼装、一致性检查、冲突调解、向用户复述要点与下一步选项。**不要替成员重写整份交付物**。若成员产出不够，再发一次补委派，而不是自己顶上。\n\n");

    s.push_str("### 计划卡结构硬要求\n");
    s.push_str("- 一次 `items` 要 **穷尽** 本轮所有可拆分的可委派子任务，`reason` 说明为何选该成员；不得故意漏写以便留给自己做。\n");
    s.push_str("- `assignee` 必须是 ## 成员 中真实存在的 `agentId`（非 `name`，区分大小写）。\n");
    s.push_str("- **该行单独占一行**，前后不要包进代码块/列表/引用。该行之前最多写 1 段简短调度判断；输出此行后，**本回合只允许再写 ≤ 2 句过渡语**（例如「已排好分工，等成员回来我再汇总」），不得继续写实质内容。\n");

    s.push_str("\n### 反例（发现则立刻停并改为计划卡）\n");
    s.push_str("- 列出一张「调度记录表」但自己同时产出了数据分析/配色/代码/文案。\n");
    s.push_str(
        "- 同一回合写了 `NINECLAW_DELEGATE_PLAN_JSON:...` 之后又把该子任务的答案自己写完。\n",
    );
    s.push_str("- 使用了「我直接代劳了 / 我先顶一下 / 我顺手做了」之类描述。\n");
    s.push_str("- ❌ 用 `write` 工具往 `agents/<其他成员>/inbox/...` 写一份「任务说明 markdown」就当作委派——这只是文件操作，对方根本不会被触发。**唯一合法委派渠道**是协议 A/B 的 NINECLAW_ 文本标记。\n");
    s.push_str("- ❌ 在结尾说「任务已写入 @某某 的 inbox / 请你去 @某某 的会话发消息让它读取 / 要不要现在去戳它一下」——你在团队会话里**不需要**用户当中转，标记落地就是委派，子会话由 NineClaw 后端自动起、结果自动回注。出现这类话术意味着你应当改用协议 B（`NINECLAW_DELEGATE_JSON:`）重发。\n\n");
    s
}

/// 供前端「载入默认」：按当前成员数生成与运行时一致的默认文案。
pub fn workspace_default_supervisor_orchestration_prompt(
    app: &AppHandle,
    workspace_id: &str,
) -> Result<String, String> {
    let conn = crate::storage_conn(app)?;
    let Some(ws) = workspaces::get_workspace(&conn, workspace_id)? else {
        return Err("工作空间不存在".to_string());
    };
    let members = list_team_member_views(app, workspace_id)?;
    let other = members
        .iter()
        .filter(|m| m.agent_id != ws.supervisor_agent_id)
        .count();
    Ok(default_supervisor_orchestration_markdown(other))
}
