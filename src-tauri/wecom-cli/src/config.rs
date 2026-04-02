#[derive(Debug, Clone)]
pub struct CategoryInfo {
    pub name: &'static str,
    pub description: &'static str,
}

/// Return all supported business categories and their tool definitions.
pub fn get_categories() -> Vec<CategoryInfo> {
    let mut categories = vec![];

    // Insert categories in alphabetical order
    categories.push(CategoryInfo {
        name: "contact",
        description: "通讯录 — 成员查询和搜索",
    });

    categories.push(CategoryInfo {
        name: "doc",
        description: "文档 — 文档/智能表格创建和管理",
    });

    categories.push(CategoryInfo {
        name: "meeting",
        description: "会议 — 创建/管理/查询视频会议",
    });

    categories.push(CategoryInfo {
        name: "msg",
        description: "消息 — 聊天列表、发送/接收消息、媒体下载",
    });

    categories.push(CategoryInfo {
        name: "schedule",
        description: "日程 — 日程增删改查和可用性查询",
    });

    categories.push(CategoryInfo {
        name: "todo",
        description: "待办事项 — 创建/查询/编辑待办项",
    });

    categories
}
