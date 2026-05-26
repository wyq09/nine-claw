const USER_MEMORY_AUTO_EXTRACTION_TEMPLATE: &str = include_str!("user_memory_auto_extraction.md");

pub fn build_user_memory_auto_extraction_prompt(
    agent_name: &str,
    recent_memories: &str,
    conversation_excerpt: &str,
) -> String {
    USER_MEMORY_AUTO_EXTRACTION_TEMPLATE
        .replace("{{agent_name}}", agent_name.trim())
        .replace("{{recent_memories}}", recent_memories.trim())
        .replace("{{conversation_excerpt}}", conversation_excerpt.trim())
}
