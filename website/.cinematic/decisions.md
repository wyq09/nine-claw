# Design Decisions

- Entry mode: Surprise me
- Genre: Sci-Fi / Thriller
- Director: Alex Garland
- Film: Ex Machina (2014)
- Niche: AI Agent Desktop Client / Developer Tool
- Pages: Homepage, Feature Details
- Major page roles: Home (establish authority and intrigue), Features (reveal the system's depth)
- Image placeholders: No
- Sub-agent delegation plan: Not available, single-agent execution

## Demo Uniqueness Audit

- Previous-work audit: No prior cinematic-ui outputs for this user. Marking as first use.
- Recurring traits to avoid: No prior history, but guard against default patterns listed below.
- Shell-ban list:
  - Centered text over gradient hero
  - Top nav + stacked card sections + footer
  - Generic three-column feature grid
  - Pill-shaped metadata rows
  - Default Tailwind blue + purple gradient
  - Rounded premium cards with subtle shadows
  - Cookie-cutter hero-left-copy-right-object layout
  - Standard sticky header with logo + links + CTA button
- Primary composition family: Glass observation chamber
  - Full-bleed stage with translucent glass panels floating in depth
  - Sections behave like observation rooms in Nathan's retreat: you look through layers
  - Content is arranged behind or within glass-like surfaces that reveal on interaction
- Why this family differs: Instead of stacking cards or splitting hero left/right, the page unfolds as a series of observation chambers where the user peers through semi-transparent layers to discover features, mirroring the film's glass-wall aesthetic.
- Wireframe-level uniqueness test: Remove all color, type, and decoration. The layout must still read as "glass chambers with depth layers" not "generic landing page with sections."

## Research Notes

### Research Boundary
- Film research is observational input, not a spec: We study how Garland builds tension through controlled reveals, how architecture conveys power dynamics, and how glass/reflective surfaces create both transparency and entrapment.
- What is being translated into web language: Glass-layer composition, shallow-focus depth hierarchy, clinical minimalism, blue/green/red color symbolism, controlled pacing with moments of revelation.
- What must not be flattened into product-template logic: The sense of discovering something hidden behind glass. The tension between what is visible and what is obscured. The feeling of peering into an intelligence.

### Research Sources
- Director source: [SYFY WIRE - How Alex Garland Uses Color](https://www.syfy.com/syfy-wire/how-alex-garland-uses-color-in-annihilation-and-ex-machina)
- Film source: [Films with Tiffany - Cinematography Analysis](https://filmswithtiffany.wordpress.com/2020/06/19/ex-machina/)
- Secondary analysis: [The Red Room - Color Symbolism](https://acelare.wordpress.com/2015/05/09/ex-machina-an-analysis-of-color-symbolism-in-film/), [Dezeen - Production Designer Interview](https://www.dezeen.com/2015/05/22/ex-machina-set-designer-mark-digby-interview-alex-garland-juvet-landscape-hotel-norway-jensen-skodvin-architects/)
- Niche source 1: [BetterYeah - AI Agent Platform Guide](https://www.betteryeah.com/blog/2026-ai-agent-platform-guide)
- Niche source 2: [Tilipman Digital - AI Website Examples](https://www.tilipmandigital.com/resource-center/articles/ai-website-examples)

### Film Palette
- Primary: `#0a0f1a` (Deep midnight blue — Caleb's grounded reality, Nathan's observation room at night)
- Secondary: `#e8ece6` (Cool clinical white — Nathan's glass walls, sterile surfaces)
- Accent: `#c4384a` (Muted crimson — power, danger, the red moment of consciousness)
- Accent secondary: `#2dd4a8` (Jade green — Ava's renewal, life, self-awareness, AI emergence)
- Shadow: `#050810` (Void black — the spaces behind glass, depth)
- Text: `#f0f0f0` (Near-white on dark), `#1a1a2e` (Dark navy on light surfaces)

### Director Signatures
1. **Controlled revelation through glass**: Scenes are framed behind transparent barriers. Web translation: glass-morphism panels that reveal content on scroll/interaction, semi-transparent card surfaces with backdrop-blur, layered z-depth.
2. **Shallow focus as power language**: Close-ups isolate the subject against blurred backgrounds. Web translation: hero elements at sharp focus with heavily blurred atmospheric backgrounds, single-element spotlight compositions.
3. **Clinical minimalism punctuated by organic intrusion**: Clean surfaces interrupted by nature (the tree visible through Ava's glass). Web translation: minimal layouts with moments of organic warmth — a curved illustration, a nature-toned accent, breathing space.

### Film Translation Notes
- Framing: Content is observed, not presented. User peers through layers to discover features. Each section feels like looking into a different room.
- Rhythm: Slow, deliberate reveals. Long holds on static compositions before a moment of movement. No rapid-fire sections.
- Lighting: Cool blue-white dominance, with warm red/green accents that feel like they're glowing from within.
- Space: Generous negative space. The void between elements is as important as the elements themselves.
- Materiality: Glass surfaces, subtle reflections, frosted edges. Backgrounds have depth (layered gradients, not flat colors).
- What should stay ambiguous or restrained: Don't over-explain every feature. Let some features remain partially obscured behind glass, revealed only on interaction. The site should feel like you're exploring a sophisticated system, not reading a brochure.

### Niche References
- URL: https://www.dezeen.com/2015/05/22/ex-machina-set-designer-mark-digby-interview-alex-garland-juvet-landscape-hotel-norway-jensen-skodvin-architects/
- URL: https://www.betteryeah.com/blog/2026-ai-agent-platform-guide

### Reference Decomposition
- Ex Machina glass architecture contributes: The observation-chamber composition family, glass-morphism surfaces, controlled reveal pacing, depth layering
- Ex Machina color language contributes: Blue (user/reality), Red (power/AI), Green (emergence/life), Clinical white (surfaces)
- Ex Machina cinematography contributes: Stationary compositions, shallow focus hierarchy, deliberate camera movement as narrative tool
- What will not be copied: Direct film stills, character imagery, movie poster layouts, plot-specific references, surveillance/voyeurism themes
