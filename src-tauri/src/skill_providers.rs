//! Skill discovery providers with an explicit numeric priority table.
//!
//! research-dsh report 03 step ②: the old code relied on implicit insertion
//! order ("first seen wins"). Now every discovery source is a
//! [`SkillProvider`] with an explicit rank; when the same skill id exists in
//! several roots, the lowest rank wins, and equal ranks keep provider
//! registration order.
//!
//! Rank table (lower = higher priority):
//! - workspace `<workspace>/skills/`           → 100
//! - workspace `<workspace>/.agents/skills/`   → 200
//! - `$CODEX_HOME/skills`                      → 300
//! - `~/.codex/skills`                         → 400
//! - `~/.agents/skills`                        → 500
//! - bundled system skills                     → 600 (merged last by caller)

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct SkillRoot {
    pub(crate) path: PathBuf,
    pub(crate) scope: &'static str,
    pub(crate) lock_path: Option<PathBuf>,
    pub(crate) rank: u32,
}

pub(crate) const RANK_WORKSPACE_SKILLS: u32 = 100;
pub(crate) const RANK_WORKSPACE_AGENTS: u32 = 200;
pub(crate) const RANK_CODEX_HOME: u32 = 300;
pub(crate) const RANK_USER_CODEX: u32 = 400;
pub(crate) const RANK_USER_AGENTS: u32 = 500;
#[allow(dead_code)] // 优先级表完整性：bundled 层由调用方直接合并，rank 保留语义锚点
pub(crate) const RANK_BUNDLED_SYSTEM: u32 = 600;

pub(crate) trait SkillProvider {
    #[allow(dead_code)] // provider 命名留给未来的诊断日志使用
    fn name(&self) -> &str;
    fn rank(&self) -> u32;
    /// Roots contributed by this provider, in provider-local priority order.
    fn roots(&self) -> Vec<SkillRoot>;
}

/// `<workspace>/skills/` — highest priority, per workspace.
struct WorkspaceSkillsProvider {
    workspace_root: PathBuf,
}

impl SkillProvider for WorkspaceSkillsProvider {
    fn name(&self) -> &str {
        "workspace/skills"
    }

    fn rank(&self) -> u32 {
        RANK_WORKSPACE_SKILLS
    }

    fn roots(&self) -> Vec<SkillRoot> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        push_skill_root(
            &mut roots,
            &mut seen,
            self.workspace_root.join("skills"),
            "workspace",
            Some(self.workspace_root.join("skills-lock.json")),
            self.rank(),
        );
        roots
    }
}

/// `<workspace>/.agents/skills/` — second workspace source.
struct WorkspaceAgentsSkillsProvider {
    workspace_root: PathBuf,
}

impl SkillProvider for WorkspaceAgentsSkillsProvider {
    fn name(&self) -> &str {
        "workspace/.agents/skills"
    }

    fn rank(&self) -> u32 {
        RANK_WORKSPACE_AGENTS
    }

    fn roots(&self) -> Vec<SkillRoot> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        push_skill_root(
            &mut roots,
            &mut seen,
            self.workspace_root.join(".agents").join("skills"),
            "workspace",
            Some(self.workspace_root.join("skills-lock.json")),
            self.rank(),
        );
        roots
    }
}

/// `$CODEX_HOME/skills` — explicit codex home directory.
struct CodexHomeProvider {
    base: PathBuf,
}

impl SkillProvider for CodexHomeProvider {
    fn name(&self) -> &str {
        "codex-home/skills"
    }

    fn rank(&self) -> u32 {
        RANK_CODEX_HOME
    }

    fn roots(&self) -> Vec<SkillRoot> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        push_skill_root(
            &mut roots,
            &mut seen,
            self.base.join("skills"),
            "global",
            None,
            self.rank(),
        );
        roots
    }
}

/// `~/.codex/skills` — user codex skills.
struct UserCodexProvider {
    home: PathBuf,
}

impl SkillProvider for UserCodexProvider {
    fn name(&self) -> &str {
        "user/.codex/skills"
    }

    fn rank(&self) -> u32 {
        RANK_USER_CODEX
    }

    fn roots(&self) -> Vec<SkillRoot> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        push_skill_root(
            &mut roots,
            &mut seen,
            self.home.join(".codex").join("skills"),
            "global",
            None,
            self.rank(),
        );
        roots
    }
}

/// `~/.agents/skills` — user agent skills, lowest filesystem priority.
struct UserAgentsProvider {
    home: PathBuf,
}

impl SkillProvider for UserAgentsProvider {
    fn name(&self) -> &str {
        "user/.agents/skills"
    }

    fn rank(&self) -> u32 {
        RANK_USER_AGENTS
    }

    fn roots(&self) -> Vec<SkillRoot> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        push_skill_root(
            &mut roots,
            &mut seen,
            self.home.join(".agents").join("skills"),
            "global",
            None,
            self.rank(),
        );
        roots
    }
}

/// All filesystem providers, registered in priority order.
pub(crate) fn candidate_skill_providers() -> Vec<Box<dyn SkillProvider>> {
    let mut providers: Vec<Box<dyn SkillProvider>> = Vec::new();

    for workspace_root in discover_workspace_roots() {
        providers.push(Box::new(WorkspaceSkillsProvider {
            workspace_root: workspace_root.clone(),
        }));
        providers.push(Box::new(WorkspaceAgentsSkillsProvider { workspace_root }));
    }

    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        providers.push(Box::new(CodexHomeProvider {
            base: PathBuf::from(codex_home),
        }));
    }

    if let Some(home) = home_dir() {
        providers.push(Box::new(UserCodexProvider { home: home.clone() }));
        providers.push(Box::new(UserAgentsProvider { home }));
    }

    providers
}

/// Flatten all provider roots, sorted by rank (ascending, stable so equal
/// ranks keep registration order). The scan pipeline deduplicates on
/// first-seen ids, so this ordering is exactly the name-resolution rule:
/// lowest rank wins.
pub(crate) fn candidate_skill_roots() -> Vec<SkillRoot> {
    let mut roots: Vec<SkillRoot> = candidate_skill_providers()
        .into_iter()
        .flat_map(|provider| provider.roots())
        .collect();
    roots.sort_by_key(|root| root.rank);
    roots
}

fn discover_workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    let mut start_points = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        start_points.push(current_dir);
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            start_points.push(parent.to_path_buf());
        }
    }

    for start_point in start_points {
        for ancestor in start_point.ancestors() {
            let candidate = ancestor.to_path_buf();
            if !looks_like_workspace_root(&candidate) {
                continue;
            }
            let canonical = fs::canonicalize(&candidate).unwrap_or(candidate.clone());
            if seen.insert(canonical) {
                roots.push(candidate);
            }
        }
    }

    roots
}

fn looks_like_workspace_root(path: &Path) -> bool {
    path.join("skills").exists()
        || path.join(".agents").join("skills").exists()
        || path.join("skills-lock.json").exists()
        || path.join("package.json").exists()
            && path.join("src-tauri").join("tauri.conf.json").exists()
}

fn push_skill_root(
    roots: &mut Vec<SkillRoot>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
    scope: &'static str,
    lock_path: Option<PathBuf>,
    rank: u32,
) {
    if !path.exists() || !path.is_dir() {
        return;
    }

    let canonical = fs::canonicalize(&path).unwrap_or(path.clone());
    if seen.insert(canonical) {
        roots.push(SkillRoot {
            path,
            scope,
            lock_path,
            rank,
        });
    }
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_table_ascends_from_workspace_to_bundled() {
        let ranks = [
            RANK_WORKSPACE_SKILLS,
            RANK_WORKSPACE_AGENTS,
            RANK_CODEX_HOME,
            RANK_USER_CODEX,
            RANK_USER_AGENTS,
            RANK_BUNDLED_SYSTEM,
        ];
        for pair in ranks.windows(2) {
            assert!(pair[0] < pair[1], "rank table must ascend: {ranks:?}");
        }
    }

    #[test]
    fn candidate_skill_roots_are_sorted_by_rank() {
        let roots = candidate_skill_roots();
        for pair in roots.windows(2) {
            assert!(pair[0].rank <= pair[1].rank, "roots must be rank-sorted");
        }
    }

    #[test]
    fn providers_assign_expected_ranks_and_skip_missing_roots() {
        let home = PathBuf::from("/tmp/fake-home");
        let codex = PathBuf::from("/tmp/fake-codex-home");
        let workspace = PathBuf::from("/tmp/fake-workspace");

        let workspace_provider = WorkspaceSkillsProvider {
            workspace_root: workspace.clone(),
        };
        assert_eq!(workspace_provider.rank(), RANK_WORKSPACE_SKILLS);
        // Missing directories contribute no roots (caller falls back cleanly).
        assert!(workspace_provider.roots().is_empty());

        let agents_provider = WorkspaceAgentsSkillsProvider { workspace_root: workspace };
        assert_eq!(agents_provider.rank(), RANK_WORKSPACE_AGENTS);

        let codex_provider = CodexHomeProvider { base: codex };
        assert_eq!(codex_provider.rank(), RANK_CODEX_HOME);

        let user_codex = UserCodexProvider { home: home.clone() };
        assert_eq!(user_codex.rank(), RANK_USER_CODEX);
        let user_agents = UserAgentsProvider { home };
        assert_eq!(user_agents.rank(), RANK_USER_AGENTS);
    }
}
