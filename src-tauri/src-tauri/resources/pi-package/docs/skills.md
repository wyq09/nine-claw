## 九节虾核心技能清单

- [x] 缓存生命周期管理 （见 module C）
- [x] invoke_skill（已补充说明）
- [ ] 子 agent 隔离执行框架
- [ ] ...

> 所有技能均可通过 `invoke_skill` 触发，支持内联轻执行 or 子 agent 隔离运行。

---
**invoke_skill 实现要点**：
1. 每个 skill 独立目录，包含 `SKILL.md`（描述）、`metadata.yaml`（配置）、`impl.bash`（可选执行脚本）
2. `invoke_skill.bash` 解析 metadata 决定是否 `fork_agent`
3. 子 agent 使用 pi-agent 的 agent_delegate 机制沙箱运行