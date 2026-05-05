# Skill Lazy Loading Optimization

Date: 2026-04-30

## Goal

Reduce per-turn token overhead caused by mounting every preferred skill for static agents, especially when an agent carries a long skill list but only needs 0-1 skills on most turns.

## What Changed

- Static agents with `0-3` preferred skills keep the old behavior: mount all preferred skills plus required system skills.
- Static agents with `4+` preferred skills now use a lazy mount path:
  - score only the agent's preferred skills against the current prompt
  - mount only the top `2` preferred skills for that turn
  - still always mount required system skills
- The runtime skill prompt injected into the model context was compressed from a multi-line strategy/reason dump to a single short line listing mounted skills.

## Heuristics

- Eager threshold: `3` preferred skills or fewer
- Lazy threshold: `4` preferred skills or more
- Lazy mount cap: `2` preferred skills per turn

## Expected Impact

- Agents that previously mounted `4+` preferred skills every turn should now mount a much smaller subset on most turns.
- Token savings come from both:
  - fewer `SKILL.md` files mounted into the runtime
  - a shorter per-turn runtime skill prompt

## Code Paths

- Skill selection: [src-tauri/src/skill_broker.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/skill_broker.rs)
- Desktop stream injection: [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs)
- IM stream injection: [src-tauri/src/channels/pi_bridge.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/channels/pi_bridge.rs)

## Tests

- `cargo test runtime_skill_prompt_formats_decision`
- `cargo test static_strategy_mounts_preferred_and_required`
- `cargo test static_strategy_with_many_skills_lazy_mounts_only_top_matches`
