# UI source provenance

- React, React DOM, Vite and the Tauri API/CLI are resolved from the npm `latest` stable tags and pinned by the lockfile.
- shadcn/ui: current components generated with CLI `4.22.1` from https://ui.shadcn.com. MIT. Shared components live in `src/packages/ui/src/ui`.
- Shaders: official `shaders` package `4.0.4`; the HUD rim uses its `shaders/core` renderer and `shaders/std` `defineShader`; https://shaders.com. The installed package's MIT license applies to the code dependency. No paid presets or account exports are used.
- MCP: official `@modelcontextprotocol/server` / `client` `2.3.1`, current stable v2 line for the `2026-07-28` specification. https://ts.sdk.modelcontextprotocol.io/v2/.

Registry-generated utility imports are adapted to `@agent-hud/ui/lib/utils` so the UI package does not import app-private modules. Shader animation is disabled with reduced motion or when the document is hidden.

- Original Agent HUD UI layouts, geometry, hover behavior and bot logic are ports from `agent-hud-native-open`, licensed under Apache-2.0. The license is included at the repository root in [LICENSE](../LICENSE). Native rendering expressions were translated to React, Canvas, SVG and Rust.
- Agent Bot release cleanup (2026-10-25): replaced the HUD content entrance transition with a small independently written `motion.div` transition using the existing Motion dependency. Removed React Bits AnimatedContent and GSAP. The previous component and license remain in private development history, which is excluded from public export. The transition still honors reduced-motion settings.
- Agent Bot 0.1.2 candidate: the NSIS template derives from Tauri CLI v2.12.3 under Apache-2.0. The experimental MCP relay and embedded Bun runtime are excluded from Windows and Mac distribution resources. The unverifiable react-remove-scroll-bar 2.3.8 source reference was replaced with 2.3.7; matching npm/upstream MIT declarations and the reproduced standard notice are recorded in release-license-supplement.txt and license-evidence.json. Unmodified MPL dependency source-download URLs remain in the supplement.
- Provider PNGs remain under src/packages/hud/src/assets with their provenance and LobeHub MIT notice. They identify the actual client sources, are not this product's logo, and do not imply provider endorsement or a trademark grant. Agent Bot 0.1.2 replaces the previous application icon and Settings wallpaper with independently generated MIT geometry; see scripts/create-release-artwork.mjs, license-map.json and artwork-provenance.json. The old icon/wallpaper are not shipped by current builds. The original orange Claude activity Bot remains under its existing Apache-2.0 terms and its static client identity is unchanged. Removed Clawd sprite-table attribution is historical provenance, not permission to redistribute that old artwork. Demo media and private history are excluded from release exports.

shadcn visual overrides live in src/packages/ui/src/global.css; registry component behavior is unchanged. HUD geometry and timelines are product implementation code. Custom Shader rim effects implement the original six glow styles and six effects; they retain a static fallback when WebGPU is unavailable.

`src/packages/vite-config/src/shader-lifecycle.ts` applies version- and source-anchor-checked
lifecycle fixes to the official shaders 4.0.4 React adapter at Vite build time.
It disposes initialization that completes after unmount and prevents duplicate
pending initialization during StrictMode effect replay. Installed upstream files remain unchanged. The HUD rim no longer uses the React adapter: `HUDGlow` owns the renderer, keeps its display-rate frame loop off and asks for frames itself, so it needs no frame-cap patch. Review/remove this temporary patch when upgrading.
