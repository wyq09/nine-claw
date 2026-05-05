use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MemoryGate {
    pub should_write: bool,
    pub decision: bool,
    pub constraint: bool,
    pub fact: bool,
    pub preference: bool,
    pub resource: bool,
    pub plan: bool,
    pub risk: bool,
    pub workflow: bool,
    pub people: bool,
    pub user_profile: bool,
    pub relationship: bool,
    pub commitment: bool,
    pub pitfall: bool,
    pub emotional_event: bool,
}

impl MemoryGate {
    pub(crate) fn from_json_value(value: &Value) -> Option<Self> {
        let gate = value.get("gate")?;
        Some(Self {
            should_write: gate
                .get("should_write")
                .and_then(Value::as_bool)
                .or_else(|| gate.get("worthy").and_then(Value::as_bool))
                .unwrap_or(false),
            decision: gate
                .get("decision")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            constraint: gate
                .get("constraint")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            fact: gate.get("fact").and_then(Value::as_bool).unwrap_or(false),
            preference: gate
                .get("preference")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            resource: gate
                .get("resource")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            plan: gate.get("plan").and_then(Value::as_bool).unwrap_or(false),
            risk: gate.get("risk").and_then(Value::as_bool).unwrap_or(false),
            workflow: gate
                .get("workflow")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            people: gate.get("people").and_then(Value::as_bool).unwrap_or(false),
            user_profile: gate
                .get("user_profile")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            relationship: gate
                .get("relationship")
                .or_else(|| gate.get("relationships"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            commitment: gate
                .get("commitment")
                .or_else(|| gate.get("commitments"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            pitfall: gate
                .get("pitfall")
                .or_else(|| gate.get("pitfalls"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            emotional_event: gate
                .get("emotional_event")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub(crate) fn inferred_should_write(&self) -> bool {
        self.should_write || self.any_route_enabled()
    }

    pub(crate) fn any_route_enabled(&self) -> bool {
        self.shared_routes().next().is_some() || self.private_routes().next().is_some()
    }

    pub(crate) fn shared_routes(&self) -> impl Iterator<Item = &'static str> {
        let mut routes = Vec::new();
        if self.decision {
            routes.push("decision");
        }
        if self.constraint {
            routes.push("constraint");
        }
        if self.fact {
            routes.push("fact");
        }
        if self.preference {
            routes.push("preference");
        }
        if self.resource {
            routes.push("resource");
        }
        if self.plan {
            routes.push("plan");
        }
        if self.risk {
            routes.push("risk");
        }
        if self.workflow {
            routes.push("workflow");
        }
        if self.people {
            routes.push("people");
        }
        routes.into_iter()
    }

    pub(crate) fn private_routes(&self) -> impl Iterator<Item = &'static str> {
        let mut routes = Vec::new();
        if self.user_profile {
            routes.push("user_profile");
        }
        if self.relationship {
            routes.push("relationship");
        }
        if self.commitment {
            routes.push("commitment");
        }
        if self.pitfall {
            routes.push("pitfall");
        }
        if self.emotional_event {
            routes.push("emotional_event");
        }
        routes.into_iter()
    }
}

pub(crate) fn canonical_memory_route(route: &str) -> Option<String> {
    match route.trim().to_ascii_lowercase().as_str() {
        "decision" | "决策" => Some("decision".to_string()),
        "constraint" | "constraints" | "约束" | "限制" => Some("constraint".to_string()),
        "fact" | "facts" | "事实" => Some("fact".to_string()),
        "preference" | "preferences" | "偏好" => Some("preference".to_string()),
        "resource" | "resources" | "资料" | "资源" => Some("resource".to_string()),
        "plan" | "plans" | "计划" => Some("plan".to_string()),
        "risk" | "risks" | "风险" => Some("risk".to_string()),
        "workflow" | "workflows" | "流程" | "工作流" => Some("workflow".to_string()),
        "people" | "person" | "成员" | "人员" => Some("people".to_string()),
        "user_profile" | "profile" | "画像" | "用户画像" => Some("user_profile".to_string()),
        "relationship" | "relationships" | "关系" | "关系图" => {
            Some("relationship".to_string())
        }
        "commitment" | "commitments" | "承诺" | "待办" => Some("commitment".to_string()),
        "pitfall" | "pitfalls" | "坑点" | "纠正" => Some("pitfall".to_string()),
        "emotional_event" | "emotion" | "affect" | "情感事件" => {
            Some("emotional_event".to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_infers_write_from_routes() {
        let gate = MemoryGate {
            preference: true,
            ..MemoryGate::default()
        };
        assert!(gate.inferred_should_write());
    }

    #[test]
    fn gate_parses_json_aliases() {
        let value = serde_json::json!({
            "gate": {
                "worthy": true,
                "relationships": true,
                "commitments": true,
                "pitfalls": true
            }
        });
        let gate = MemoryGate::from_json_value(&value).expect("gate");
        assert!(gate.inferred_should_write());
        assert!(gate.relationship);
        assert!(gate.commitment);
        assert!(gate.pitfall);
    }

    #[test]
    fn canonical_route_supports_shared_and_private_routes() {
        assert_eq!(
            canonical_memory_route("workflow").as_deref(),
            Some("workflow")
        );
        assert_eq!(
            canonical_memory_route("用户画像").as_deref(),
            Some("user_profile")
        );
        assert_eq!(
            canonical_memory_route("情感事件").as_deref(),
            Some("emotional_event")
        );
    }
}
