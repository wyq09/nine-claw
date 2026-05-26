const SESSION_COMPRESSION_TEMPLATE: &str = include_str!("session_compression.md");

pub fn build_session_compression_prompt(
    compression_level: u64,
    target_tokens: u64,
    level_guidance: &str,
) -> String {
    SESSION_COMPRESSION_TEMPLATE
        .replace("{{compression_level}}", &compression_level.to_string())
        .replace("{{target_tokens}}", &target_tokens.to_string())
        .replace("{{level_guidance}}", level_guidance.trim())
}
