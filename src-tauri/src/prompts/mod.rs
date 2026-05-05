//! LLM / 系统指令文案集中存放，便于审阅与迭代。

mod desktop_media_reply;
mod session_title;
mod task_metadata;
mod user_kv_memory_reorganize;
mod user_memory_auto_extraction;
mod workspace_memory_extraction;

pub use desktop_media_reply::desktop_media_reply_prompt;
pub use session_title::build_session_title_prompt;
pub use task_metadata::build_task_metadata_prompt;
pub use user_kv_memory_reorganize::build_user_kv_memory_reorganize_prompt;
pub use user_memory_auto_extraction::build_user_memory_auto_extraction_prompt;
pub use workspace_memory_extraction::build_workspace_memory_extraction_prompt;
