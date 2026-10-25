# UI source provenance

- React, React DOM, Vite and the Tauri API/CLI are resolved from the npm `latest` stable tags and pinned by the lockfile.
- shadcn/ui: current components generated with CLI `4.22.1` from https://ui.shadcn.com. MIT. Shared components live in `src/packages/ui/src/ui`.
- Shaders: official `shaders` package `4.0.4`; the HUD rim uses its `shaders/core` renderer and `shaders/std` `defineShader`; https://shaders.com. The installed package's MIT license applies to the code dependency. No paid presets or account exports are used.
- MCP: official `@modelcontextprotocol/server` / `client` `2.3.1`, current stable v2 line for the `2026-07-28` specification. https://ts.sdk.modelcontextprotocol.io/v2/.

Registry-generated utility imports are adapted to `@agent-hud/ui/lib/utils` so the UI package does not import app-private modules. Shader animation is disabled with reduced motion or when the document is hidden.

- Original Agent HUD UI layouts, geometry, hover behavior and bot logic are ports from `agent-hud-native-open`, licensed under Apache-2.0. The license is included at the repository root in [LICENSE](../LICENSE). Native rendering expressions were translated to React, Canvas, SVG and Rust.
- Agent Bot release cleanup (2026-10-25): replaced the HUD content entrance transition with a small independently written `motion.div` transition using the existing Motion dependency. Removed React Bits AnimatedContent and GSAP. The previous component and license remain in private development history, which is excluded from public export. The transition still honors reduced-motion settings.
- Agent Bot 0.1.1: the NSIS template is derived from Tauri CLI v2.12.3 under Apache-2.0, with environment checks and WebView2 recovery changes documented beside the template. The optional MCP relay (including its embedded Bun/JavaScriptCore runtime) is excluded from the Windows installer; it is still built for development. Additional fetched notices and unmodified MPL dependency source-download URLs are in `release-license-supplement.txt`. The remaining version/license correspondence gap and artwork review are recorded in `docs/release/ACCEPTANCE.md` in the outer distribution repository.
- Original provider PNGs remain under src/packages/hud/src/assets with their notices; the Settings wallpaper is in src/packages/ui/src/assets and the application icon in src/packages/app/src/assets. Agent Hub removed the extracted Clawd sprite table on 2026-10-25 and uses independently drawn orange Bot pixels from src/packages/hud/src/bots/seed-robot.ts under Apache-2.0, with a matching procedural fallback. The Bot retains Claude source identity and does not introduce a separate mascot brand. Existing Clawd API names describe upstream animation contracts only. The upstream artwork notice is retained as historical provenance; see src/packages/hud/src/assets/README.md and THIRD_PARTY_NOTICES.txt. Upstream documentation demo media and private Git history predate this replacement and must be excluded or separately reviewed for public export.

shadcn visual overrides live in src/packages/ui/src/global.css; registry component behavior is unchanged. HUD geometry and timelines are product implementation code. Custom Shader rim effects implement the original six glow styles and six effects; they retain a static fallback when WebGPU is unavailable.

`src/packages/vite-config/src/shader-lifecycle.ts` applies version- and source-anchor-checked
lifecycle fixes to the official shaders 4.0.4 React adapter at Vite build time.
It disposes initialization that completes after unmount and prevents duplicate
pending initialization during StrictMode effect replay. Installed upstream files remain unchanged. The HUD rim no longer uses the React adapter: `HUDGlow` owns the renderer, keeps its display-rate frame loop off and asks for frames itself, so it needs no frame-cap patch. Review/remove this temporary patch when upgrading.
