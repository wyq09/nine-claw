//! Skill manifest parsing (frontmatter + body fallbacks).
//!
//! Moved out of `skills.rs` so the discovery pipeline and the manifest
//! grammar can evolve independently. Works for both bundle (`SKILL.md`) and
//! flat (`<name>.md`) skill formats.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SkillManifest {
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) triggers: Vec<String>,
    pub(crate) examples: Vec<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) requires_auth: bool,
    pub(crate) side_effect_level: Option<String>,
    pub(crate) modes: Vec<String>,
}

pub(crate) fn parse_skill_manifest(content: &str) -> SkillManifest {
    let mut manifest = SkillManifest::default();
    let mut body_lines = Vec::new();
    let mut lines = content.lines().peekable();

    if matches!(lines.peek(), Some(line) if line.trim() == "---") {
        lines.next();
        let mut frontmatter_lines = Vec::new();
        let mut block_scalar_indent: Option<usize> = None;
        for line in lines.by_ref() {
            let trimmed = line.trim();
            let indent = line.chars().take_while(|char| char.is_whitespace()).count();

            if let Some(active_indent) = block_scalar_indent {
                if trimmed.is_empty() {
                    frontmatter_lines.push(line.to_string());
                    continue;
                }
                if indent > active_indent {
                    frontmatter_lines.push(line.to_string());
                    continue;
                }
                block_scalar_indent = None;
            }

            if trimmed == "---" {
                break;
            }
            if let Some((_, value)) = trimmed.split_once(':') {
                let value = value.trim();
                if matches!(value, "|" | "|-" | "|+" | ">" | ">-" | ">+") {
                    block_scalar_indent = Some(indent);
                }
            }
            frontmatter_lines.push(line.to_string());
        }

        if let Some(frontmatter) = parse_frontmatter(&frontmatter_lines.join("\n")) {
            manifest = frontmatter;
        }
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        body_lines.push(trimmed.to_string());
    }

    if manifest.name.is_none() {
        manifest.name = body_lines
            .iter()
            .find_map(|line| {
                if line.starts_with('#') {
                    Some(line.trim_start_matches('#').trim().to_string())
                } else {
                    None
                }
            })
            .filter(|value| !value.is_empty());
    }

    if manifest.description.is_none() {
        manifest.description = body_lines
            .iter()
            .find(|line| {
                !line.starts_with('#')
                    && !line.starts_with("```")
                    && !line.starts_with('-')
                    && !line.starts_with('*')
            })
            .map(|line| line.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    manifest
}

fn parse_frontmatter(content: &str) -> Option<SkillManifest> {
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let map = parsed.as_mapping()?;
    Some(SkillManifest {
        name: read_yaml_string(map, "name"),
        description: read_yaml_string(map, "description"),
        triggers: read_yaml_string_list(map, "triggers"),
        examples: read_yaml_string_list(map, "examples"),
        capabilities: read_yaml_string_list(map, "capabilities"),
        requires_auth: read_yaml_bool(map, "requiresAuth"),
        side_effect_level: read_yaml_string(map, "sideEffectLevel"),
        modes: read_yaml_string_list(map, "modes"),
    })
}

fn read_yaml_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(serde_yaml::Value::String(key.to_string()))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_yaml_bool(map: &serde_yaml::Mapping, key: &str) -> bool {
    map.get(serde_yaml::Value::String(key.to_string()))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn read_yaml_string_list(map: &serde_yaml::Mapping, key: &str) -> Vec<String> {
    let Some(value) = map.get(serde_yaml::Value::String(key.to_string())) else {
        return Vec::new();
    };

    if let Some(single) = value.as_str() {
        return vec![single.trim().to_string()]
            .into_iter()
            .filter(|item| !item.is_empty())
            .collect();
    }

    value
        .as_sequence()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_prefers_frontmatter_fields() {
        let manifest = parse_skill_manifest(
            r#"---
name: all-plan
description: "Collaborative planning using abstract roles."
---

# Ignored title

Fallback description
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("all-plan".to_string()),
                description: Some("Collaborative planning using abstract roles.".to_string()),
                ..SkillManifest::default()
            }
        );
    }

    #[test]
    fn parse_manifest_falls_back_to_heading_and_body() {
        let manifest = parse_skill_manifest(
            r#"
# Browser Skill

Automate browser interactions for data collection.
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("Browser Skill".to_string()),
                description: Some("Automate browser interactions for data collection.".to_string()),
                ..SkillManifest::default()
            }
        );
    }

    #[test]
    fn parse_manifest_supports_literal_multiline_description() {
        let manifest = parse_skill_manifest(
            r#"---
name: multiline-skill
description: |
  第一行简介
  ---
  第二行才是补充说明
metadata:
  short-description: ignored
---
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("multiline-skill".to_string()),
                description: Some("第一行简介\n---\n第二行才是补充说明".to_string()),
                ..SkillManifest::default()
            }
        );
    }

    #[test]
    fn parse_manifest_supports_folded_multiline_description() {
        let manifest = parse_skill_manifest(
            r#"---
name: folded-skill
description: >
  第一行简介
  第二行继续补充

  第二段说明
---
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("folded-skill".to_string()),
                description: Some("第一行简介 第二行继续补充\n第二段说明".to_string()),
                ..SkillManifest::default()
            }
        );
    }

    #[test]
    fn parse_manifest_reads_dynamic_skill_metadata() {
        let manifest = parse_skill_manifest(
            r#"---
name: pptx
description: Create slides
triggers:
  - slides
  - presentation
examples:
  - make a deck
capabilities:
  - export pptx
requiresAuth: true
sideEffectLevel: high
modes:
  - worker
  - supervisor
---
"#,
        );

        assert_eq!(manifest.name.as_deref(), Some("pptx"));
        assert_eq!(
            manifest.triggers,
            vec!["slides".to_string(), "presentation".to_string()]
        );
        assert_eq!(manifest.examples, vec!["make a deck".to_string()]);
        assert_eq!(manifest.capabilities, vec!["export pptx".to_string()]);
        assert!(manifest.requires_auth);
        assert_eq!(manifest.side_effect_level.as_deref(), Some("high"));
        assert_eq!(
            manifest.modes,
            vec!["worker".to_string(), "supervisor".to_string()]
        );
    }
}
