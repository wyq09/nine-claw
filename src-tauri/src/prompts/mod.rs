//! LLM / 系统指令文案集中存放，便于审阅与迭代。

mod desktop_media_reply;
mod session_title;
mod task_metadata;

pub use desktop_media_reply::desktop_media_reply_prompt;
pub use session_title::build_session_title_prompt;
pub use task_metadata::build_task_metadata_prompt;
