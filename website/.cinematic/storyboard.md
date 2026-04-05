# Director's Treatment

## Director Brief
- Visual thesis: "The page is an observation chamber — you peer through glass layers to discover an intelligence living within"
- Signature technique 1: Glass-morphism surfaces with backdrop-blur, revealing content behind translucent panels (translating Nathan's glass walls)
- Signature technique 2: Shallow-focus depth hierarchy — one element razor-sharp, background atmosphere heavily blurred (translating Rob Hardy's shallow DoF)
- Signature technique 3: Color-coded identity — blue for user, green for AI emergence, red for power moments (translating film's color symbolism)
- Motion rules: Slow, deliberate reveals. No rapid-fire animations. Long holds on compositions. Movement feels like a camera slowly pushing in, not an editor cutting.
- Typography rules: Geometric sans-serif for UI elements (clinical), one elegant serif for headlines (Nathan's refined taste). Generous letter-spacing on labels. Ultra-light weights for ambient text, bold for commands.

## Site Cinematic Grammar
- Page-shell logic: Full-bleed dark canvas with floating glass panels. No visible container boundaries — sections bleed into each other with atmospheric transitions.
- Navigation posture: Minimal floating nav, transparent, becomes frosted glass on scroll. Logo left, few links right. No heavy nav bar — it should feel invisible until needed.
- Framing discipline: Content is observed through frames. Every major content block lives within or behind a glass surface with subtle border and blur.
- Density cadence: Alternating between dense feature reveals and vast breathing room. Hero = sparse. Features = dense. Stats = sparse. Channels = dense. CTA = sparse.
- Recurring material layers: (1) Deep gradient background with subtle mesh/noise, (2) Frosted glass panels floating above, (3) Sharp content on glass surfaces, (4) Accent glow elements (green/red/blue) at key moments.
- Allowed composition families: Glass observation chamber, asymmetric weight, editorial stack.
- What may repeat: Glass surface treatment, accent color positions, navigation style, footer structure.
- What must vary page to page: Hero composition, section entrance patterns, dominant visual element per section, rhythm of dense/sparse.
- Demo uniqueness guardrail: Must not collapse into generic dark-mode SaaS page with gradient hero and card grid.

## Page Arc — Homepage

Director: Alex Garland
Arc variant: Default (derived from Ex Machina pacing)
Beat count: 7

| Beat # | Beat Name | → Function | Director Justification |
|--------|-----------|------------|----------------------|
| 1 | B2 Establishing Shot | Hero — Atmosphere Bath variant | Ex Machina opens with helicopter over vast landscape, establishing isolation and scale before the first word is spoken |
| 2 | B7 The Encounter | Featured Feature — AI Agent | Caleb first meets Ava through glass — the first encounter with the intelligence behind the surface |
| 3 | B10 Deep Dive | Process/Steps — How It Works | Nathan explains his process to Caleb — methodical, clinical, step-by-step reveal of the system |
| 4 | B14 The Pivot | Visual Break — Channel Reveal | The film pivots from confined rooms to the exterior forest — sudden expansion of possibility |
| 5 | B8 Evidence Wall | Stats Counter — Capabilities | Nathan's workshop reveals the history of iterations — overwhelming evidence of capability |
| 6 | B20 The Invitation | CTA — Download | Ava's final invitation to act — the moment of choosing to engage |
| 7 | B22 The Farewell | Footer | The film's final shot — standing at the intersection, looking out. Quiet ending. |

## Page Arc — Features

Director: Alex Garland
Arc variant: Deep exploration
Beat count: 7

| Beat # | Beat Name | → Function | Director Justification |
|--------|-----------|------------|----------------------|
| 1 | B3 The Promise | Hero — What NineClaw Delivers | "I think you'll find interesting" — Nathan's invitation, promising something worth the journey |
| 2 | B6 World Exploration | Category Map — Feature Categories | The tour of Nathan's facility — discovering rooms with different purposes |
| 3 | B11 The Tutorial | Process/Steps — Architecture | Nathan explaining the technical architecture — clinical, precise, authoritative |
| 4 | B14 The Pivot | Visual Break — Multi-Channel | Transitioning from the lab to the outside world — connecting to channels beyond |
| 5 | B8 Evidence Wall | Comparison Table — Model Support | The wall of previous iterations — evidence of depth and range |
| 6 | B19 Quiet Moment | Testimonial / Philosophy | A quiet reflection on what AI agents mean — philosophy before action |
| 7 | B20 The Invitation | CTA — Get Started | The invitation to begin using the system |

---

## Page: Home

- Page-role scene: The Glass Chamber — first encounter with an intelligence
- Page scene thesis: You step into a dark, refined observation room. Through glass panels, you glimpse an AI agent system of extraordinary capability. Each panel reveals another facet — agents, channels, models. The experience builds from mystery to understanding to desire.
- One big idea: **Translucent discovery** — features are discovered through glass, not listed in cards
- Hero dominance statement: A vast dark canvas with a single frosted glass panel in the center, behind which a jade-green glow pulses like the first sign of consciousness — the user peering into the observation chamber and seeing something alive
- Restraint statement: No feature screenshots, no product mockups, no typical SaaS hero objects. The glass panel and its glow IS the product preview.
- Material thesis: Frosted glass surfaces with 10-20px backdrop-blur, subtle 1px borders at 15% opacity, deep navy backgrounds with radial gradient fog. Accent glows are soft, not sharp neon.
- Typography thesis: Headlines in a refined serif (weight 400-500), body in geometric sans. Generous negative space around text. Key phrases can be highlighted with the jade accent underline.
- Narrative arc: Establishing Shot → Encounter → Deep Dive → Pivot → Evidence → Invitation → Farewell
- Hero archetype: #18 Framed Viewport (adapted) — the glass observation window
- Signature composition: Central frosted glass panel floating in dark void, with jade-green ambient glow behind it. Content appears ON the glass surface. The panel has subtle corner brackets. Background has deep gradient with faint grid texture.
- Grid fallback test: If reduced to a generic grid, the glass-depth layering is lost. The user would see a dark page with text blocks instead of peering through an observation window at an intelligence. The discovery-through-glass metaphor breaks entirely.
- Shared system holdback: Navigation, footer, spacing rhythm, button styles — lock only after both pages are designed.
- UI exposure guardrail: No director names, no film references, no "chapter" labels, no "observation chamber" text in the UI.
- What this page must not inherit from previous demos: Generic dark gradient hero with centered text, three-column feature cards, typical sticky nav with hamburger menu.
- Section sequence:

### Scene 1: Hero (B2 Establishing Shot)
- Beat: Establishing Shot — set the world before any characters speak
- Function: Hero #18 Framed Viewport (adapted)
- Archetype: Central glass panel in dark void
- Composition: Centered glass panel (600px max-width) with jade glow behind, floating in a 100vh dark canvas. Navigation minimal top-left logo + top-right links.
- Entrance: Content fades in slowly from void (2s). First the background gradient, then the glass panel materializes, then text appears on the glass.
- Interaction: Mouse movement subtly shifts the glass panel position (parallax, 5-10px). The jade glow responds to cursor proximity.
- Visual elements: Gradient fog layer, floating orb (jade), corner bracket frames on glass panel
- Copy: Headline on glass: "你的 AI 智能体，不止于对话". Subtitle: "NineClaw — 多模型、多渠道、多智能体的桌面客户端"
- Why this exists: Sets the cinematic tone. Establishes the glass-chamber metaphor. Invites curiosity.

### Scene 2: The Encounter (B7 — AI Agents)
- Beat: The Encounter — first deep connection with the intelligence
- Function: Featured Feature — Agent System
- Archetype: Left-right asymmetric (70/30). Left: glass panel with agent description. Right: floating agent identity cards behind glass.
- Entrance: Glass panels slide in from left and right (staggered 0.3s).
- Interaction: Hover on agent cards causes subtle glow shift.
- Visual elements: Glass cards with agent names (主代理, 研究员, 执行者), floating behind frosted surface.
- Copy: "自定义智能体 / 每个智能体拥有独立的身份、记忆和技能 / 对话式创建，而非冷冰冰的表单"
- Why this exists: The first "meeting" with the AI agents — the core product experience.

### Scene 3: Deep Dive (B10 — How It Works)
- Beat: Deep Dive — go deep into how the system works
- Function: Process/Steps — Architecture overview
- Archetype: Full-width horizontal timeline with 4 steps, each behind its own glass panel
- Entrance: Steps reveal sequentially on scroll (each triggers when 30% visible)
- Interaction: None (restful section)
- Visual elements: Glass panels connected by thin lines, subtle green flow animation between steps
- Copy: Steps: "选择模型 → 创建智能体 → 接入渠道 → 自动执行"
- Why this exists: Clinical, methodical explanation — like Nathan explaining his creation process.

### Scene 4: The Pivot (B14 — Multi-Channel)
- Beat: The Pivot — sudden tonal shift, expansion of possibility
- Function: Visual Break + Channel showcase
- Archetype: Full-bleed section with floating channel badges, breaking from the glass-panel rhythm
- Entrance: Channels cascade in with staggered timing (0.05s apart)
- Interaction: Hover reveals channel detail tooltip
- Visual elements: Channel badges (微信, 飞书, 钉钉, 企业微信) as floating glass chips
- Copy: "一个智能体，多个通道 / 将 AI 能力延伸到你日常使用的每一个通讯工具"
- Why this exists: The pivot from confined product to expansive possibility — like moving from Nathan's lab to the outside world.

### Scene 5: Evidence Wall (B8 — Capabilities)
- Beat: Evidence Wall — overwhelming proof of capability
- Function: Stats Counter + Feature highlights
- Archetype: Grid of stat cards on glass surfaces
- Entrance: Counter animation on scroll-into-view
- Interaction: None (data speaks for itself)
- Visual elements: Large numbers with glass backgrounds, faint grid texture
- Copy: Stats: "6+ AI模型" "4+ 通讯渠道" "自定义技能" "对话式创建" "定时任务" "本地运行"
- Why this exists: The evidence that this system is real and capable — like Nathan's workshop of iterations.

### Scene 6: The Invitation (B20 — CTA)
- Beat: The Invitation — gentle, not demanding
- Function: CTA / Download
- Archetype: Centered composition with glass button
- Entrance: Fade in
- Interaction: Button has subtle magnetic pull toward cursor
- Visual elements: Glass button with jade glow border, download icon
- Copy: "开始使用 NineClaw / 下载桌面客户端，免费体验 AI 智能体的全部能力"
- Why this exists: The invitation to cross from observation to participation.

### Scene 7: The Farewell (B22 — Footer)
- Beat: The Farewell — end credits, respectful closure
- Function: Footer
- Archetype: Minimal footer, dark, sparse
- Entrance: Visible on scroll
- Interaction: None
- Visual elements: Logo, links, subtle divider line
- Copy: "NineClaw © 2024-2026 / GitHub / 文档 / 联系"
- Why this exists: The quiet ending — like the film's final intersection shot.

---

## Page: Features

- Page-role scene: The Laboratory Tour — deep exploration of the system
- Page scene thesis: A guided tour through Nathan's facility — each room reveals a different capability, building understanding through methodical, clinical presentation.
- One big idea: **Layered revelation** — features are revealed in layers, each section peels back another layer of the system
- Hero dominance statement: A wide-angle view of the system architecture rendered as translucent layers stacked with depth — the user immediately sees the complexity and elegance of the whole system.
- Restraint statement: No animated demos, no video walkthroughs. Let the architecture speak through clean composition and precise typography.
- Material thesis: Same glass surfaces but slightly more transparent (backdrop-blur 8px) — the "lab" feels more revealing than the "observation chamber."
- Typography thesis: Same system but headlines are larger (clamp 2.5rem-4rem), body text is denser — more information, presented with authority.
- Narrative arc: Promise → Exploration → Tutorial → Pivot → Evidence → Reflection → Invitation
- Hero archetype: #15 Asymmetric Weight (70/30) — the system overview
- Signature composition: A full-width glass "blueprint" — the system architecture rendered as a layered diagram on a translucent surface, asymmetric with the diagram dominant and text supporting.
- Grid fallback test: If reduced to generic grid, the layered architecture diagram loses its depth metaphor. It becomes a list of features instead of a spatial exploration of the system.
- Shared system holdback: Same as homepage.
- UI exposure guardrail: Same as homepage — no process language in UI.
- What this page must not inherit: Same hero composition as homepage, same glass-panel-for-every-section rhythm.

- Section sequence:

### Scene 1: Hero (B3 The Promise)
- Beat: The Promise — what you'll discover
- Function: Hero #15 Asymmetric Weight
- Composition: 70% left: system architecture diagram (CSS-rendered glass layers). 30% right: headline + promise text.
- Entrance: Architecture layers slide in from left, text fades in from right.
- Copy: "深入了解 NineClaw 的每一个细节 / 从智能体架构到渠道集成，探索系统的全部能力"
- Why this exists: Promises depth before delivering it.

### Scene 2: World Exploration (B6 — Feature Categories)
- Beat: World Exploration — browse the breadth
- Function: Category Map with tabs
- Composition: Horizontal tab bar with 4 categories. Content area below shows features for selected category on a glass surface.
- Categories: "智能体" "对话引擎" "渠道接入" "技能系统"
- Entrance: Tab bar slides down, default content fades in.
- Interaction: Tab switch with crossfade.
- Why this exists: Let the user explore at their own pace — like wandering through Nathan's facility rooms.

### Scene 3: Tutorial (B11 — Architecture)
- Beat: The Tutorial — how it all works
- Function: Process/Steps — detailed architecture
- Composition: Vertical stack of numbered steps, each with a code/architecture snippet and description.
- Entrance: Each step reveals on scroll with staggered timing.
- Interaction: Code blocks have subtle syntax highlight on hover.
- Copy: Technical architecture details for each layer (Agent → PiBridge → Channel → Provider)
- Why this exists: Nathan's clinical explanation moment — technical authority.

### Scene 4: Pivot (B14 — Multi-Channel Detail)
- Beat: The Pivot — channels expand the world
- Function: Visual Break + Detailed channel cards
- Composition: Full-bleed with floating channel detail panels, each showing protocol specifics.
- Entrance: Cards cascade in with stagger.
- Copy: Each channel (微信/飞书/钉钉/企微) with setup details and capabilities.
- Why this exists: The moment of expansion — connecting the system to the real world.

### Scene 5: Evidence (B8 — Model Support)
- Beat: Evidence Wall — proof of range
- Function: Comparison Table — supported models
- Composition: Glass table with model comparison (provider, model name, capabilities)
- Entrance: Fade in on scroll.
- Models: OpenAI, Anthropic, DeepSeek, Doubao, SiliconFlow
- Why this exists: Demonstrating breadth of support — the wall of capability.

### Scene 6: Quiet Moment (B19 — Philosophy)
- Beat: Quiet Moment — reflection
- Function: Testimonial / Philosophy statement
- Composition: Single centered quote on a clean glass panel, generous whitespace.
- Entrance: Slow fade (1.5s).
- Copy: A philosophy statement about AI agents: "智能体不该是冷冰冰的配置表。它应该有身份、有记忆、有个性 — 很快出生，很快能干活。"
- Why this exists: Breath before the CTA. Emotional resonance.

### Scene 7: The Invitation (B20 — Get Started)
- Beat: The Invitation
- Function: CTA — same pattern as homepage
- Composition: Centered CTA on glass panel.
- Copy: "开始探索 / 查看文档 或 直接下载"
- Why this exists: The gentle invitation to act.
