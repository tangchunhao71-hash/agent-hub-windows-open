# Agent Hub artwork and upstream provider marks

The provider marks in this folder are unchanged copies from
`agent-hud-native-open/Sources/AgentHUDDesktop/Resources/`.

The 22 provider PNGs are the exact files named by the original desktop
`Components/AgentLogo.swift` artwork map, including the OpenCode and Qoder dark
variants. Codex and ChatGPT share `chatgpt.png`. In the original renderer,
ChatGPT's mark is tinted `#10a37f`; Grok, Cursor, Kimi, Pi, GitHub Copilot, Hermes
and ZCode are template images tinted to their foreground. The source bitmaps
are preserved here without applying those rendering transformations.

Agent Hub removed the extracted `clawd-sprites.json` on 2026-10-25. Claude's
animated character is now an original round, soft orange Bot with short limbs. Every pixel is generated from new geometry in
`../bots/seed-robot.ts`, under Apache-2.0. The small-render fallback in
`../bots/bot-spec.ts` uses the same original silhouette. No Anthropic sprite
table is loaded or shipped by the current renderer.

The existing `Clawd*` internal API names refer to the Apache-licensed upstream
animation timing and state machine, retained for compatibility. They do not
indicate that the old artwork is still present. The upstream Clawd notice is
retained as a historical provenance record in `THIRD_PARTY_NOTICES.txt`.

Provider marks retain their respective owners' branding. The original
LobeHub icon notice and full MIT text are preserved in
`LobeIcons-LICENSE.txt`. `THIRD_PARTY_NOTICES.txt` retains the upstream attribution
record plus an explicit note about the removed artwork; its code-parser notices describe the
original application and do not imply that all of those parsers are used by
this UI package.

The orange Bot deliberately changes the character artwork; its appearance does not
claim visual parity with Clawd. Provider marks keep their respective branding.
