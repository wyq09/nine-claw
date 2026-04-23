use std::fs::read_to_string;
use std::path::PathBuf;

#[test]
fn team_supervisor_orchestrator_skill_manifest_has_expected_metadata() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("system-skills")
        .join("team-supervisor-orchestrator")
        .join("SKILL.md");

    let manifest = read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("read {} failed: {error}", manifest_path.display()));

    assert!(manifest.starts_with("---\nname: team-supervisor-orchestrator\n"));
    assert!(manifest.contains("description: 在 NineClaw 团队空间内作为主 Agent 进行成员能力识别、复杂任务拆解、子 Agent 委派与结果汇总的协作技能。"));
    assert!(manifest.contains("NINECLAW_DELEGATE_PLAN_JSON"));
    assert!(manifest.contains("NINECLAW_DELEGATE_JSON"));
}
