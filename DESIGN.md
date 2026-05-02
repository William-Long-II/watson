# Watson — Design Direction

> One-line north star: **The first truly cross-platform, OSS, automation-native launcher platform.**

This document records the product direction Watson is committing to. New
features should be filterable through it: if a proposal doesn't move us
closer to this picture, it doesn't ship.

The audit and competitive analysis that produced this direction are
captured at the bottom of this file for reference.

---

## What Watson is

A keyboard-driven launcher that:

- **Runs on macOS, Windows, and Linux from the same codebase** with full
  feature parity on Mac/Windows. Linux ships with documented caveats
  (Wayland window-management is wlroots-only; AT-SPI browser tabs are
  per-browser flaky) but is not a second-class consumer.
- **Is open source and BYO-API-key.** No SaaS lock-in, no Pro tier
  gating AI access, no opaque telemetry. Cloud sync is optional and
  self-hostable.
- **Is automation-native.** First-class Scenes ("Start Work" — open my
  apps, browser tabs, and windows in one stroke). First-class MCP host
  (call your tools from any AI prompt). User-defined chains of actions
  without writing a workflow language from scratch.
- **Stays consumer-friendly.** Sensible defaults, a real Settings UI,
  no shell scripting required for basic automation. Power-user features
  are reachable but not the entry point.

## What Watson is *not*

- An "app launcher with features bolted on." We're past that, and the
  audit (below) showed the architectural cost. New features must reduce
  debt or unlock a class — not add one leaf.
- A SaaS product. We won't run a backend the user depends on. Sync, AI
  routing, extensions delivery — all designed for self-hosting or
  local-first operation.
- A workflow language. Alfred shipped bash-script Workflows in 2013 and
  it shows. We compose typed Actions; no DSL, no node graph editor in
  v1.
- A dev-tools-only product. The default UX should make sense for a
  non-developer who reads "Start Work" and goes "yes, that."

## The seat we're claiming

| | Raycast | Alfred | Flow Launcher | Spotlight (Tahoe) | **Watson** |
|---|---|---|---|---|---|
| Cross-platform Mac+Win | ⚠️ Windows beta, partial | ❌ Mac only | ❌ Win only | ❌ Mac only | ✅ |
| OSS / BYO-key | ❌ Pro tier | ❌ paid | ✅ MIT | ✅ OS | ✅ |
| MCP-native | ✅ | ❌ | ❌ | ❌ | ⏳ Phase 2b |
| Scenes / Start-Work primitive | ❌ | ⚠️ via shell workflows | ❌ | ❌ | ⏳ Phase 2a |
| Modern extension language | TS | bash/PHP/AppleScript | C#/Python/JS | n/a | ⏳ Phase 2c |
| Window management built-in | ✅ | plugin | plugin | ✅ | ✅ |
| Cross-OS browser tab switching | ✅ Mac, ⚠️ Win beta | ❌ | ❌ | ❌ | ✅ |

The one square nobody else fills: a *single, polished, cross-platform,
open-source* launcher that ships Scenes and MCP first-class. Raycast
Windows is too far behind their Mac product to compete on parity in the
near term; Alfred has no Windows answer; Flow has neither AI/MCP nor
macOS. Spotlight ate the floor (clipboard, snippets, app intents are
free in macOS 26 Tahoe), so we have to live above the floor — Scenes
and MCP are above it.

## Architectural shape

The audit identified Watson as a *dispatcher with hardcoded providers,
not a platform.* The Phase 1 refactor below is the precondition for
everything that follows.

The target shape, in order of dependency:

1. **`ResultProvider` registry.** One trait, one impl per searchable
   thing (apps, files, notes, snippets, windows, tabs, scenes, web,
   system commands, captures). The dispatcher iterates the registry
   instead of hand-mapping eight bespoke struct-literal blocks in
   `lib.rs::search`. Adding a kind = adding a file.

2. **`Action { kind, payload }` + handler registry.** Replaces the
   closed `SearchAction` enum. Actions become first-class values
   addressable by id; secondary actions ("Reveal in folder", "Copy as
   Markdown") attach to `ResourceKind` via the registry rather than
   needing a new enum variant per resource.

3. **Resource graph.** Every searchable thing is a `Resource` with a
   `kind`, a `title`, optional `metadata`, and a list of applicable
   `Actions`. The current `SearchResult` shape is one rendering of a
   Resource; future renderings (a per-Resource detail panel, hover
   previews) live on the same data.

4. **Panel host.** Frontend collapses five mutually-exclusive
   `*Visible` booleans into one tagged union (`currentPanel: PanelId
   | null`) and a `<PanelHost>` component. The next panel costs zero.

5. **Capture model.** Notes, scratchpad, clipboard, snippets are 4/13
   features that hold "user text" with zero shared abstraction. They
   collapse behind `trait Capture { id, kind, content, created, tags
   }`. "Search across all my captured text" becomes a one-liner.

6. **Extension API (Phase 3).** TypeScript modules that register
   Resources + Actions; distributed via a Git-based registry; no
   marketplace yet. The trait surface from steps 1+2 is what
   extensions plug into.

## Roadmap

Sequence matters. Each phase pays down debt OR ships a flagship — never
just adds a leaf feature.

### Phase 0 — Bug-fix housekeeping (in flight)

- ✅ v1.6.1 patch: snippet-paste reliability on Windows (#71) and macOS
  parity via CGEvent

### Phase 1 — Pay down debt (target: 4-6 weeks)

- **Extract `ResultProvider` trait + registry.** Replace the bespoke
  blocks in `lib.rs::search`. Migrate providers one at a time.
- **Collapse `SearchAction` to `Action { kind, payload }` +
  handlers.** Pair with `ResourceKind`-keyed dispatch for secondary
  actions.
- **Unify panels behind `currentPanel: PanelId | null`.**
- **Cuts.** Three concrete cuts to commit:
  - Scratchpad collapses into "untitled note" (one mode of `notes`).
  - Window-management commands leave `system_commands` and become
    their own top-level concept.
  - Startup-warning banner + notifications drawer collapse to a
    single drawer-as-history surface.

### Phase 2a — Scenes (first flagship, target: 3-4 weeks after Phase 1)

A **Scene** is a named, ordered list of Actions. Activating a Scene
runs its actions sequentially with a small inter-step delay. Built on
the new `Action` handler registry from Phase 1 — Scenes are
*compositions* of actions, not a parallel pipeline.

V1 scope:

- Scene resource: `id`, `name`, `steps: [Action]`, persisted to SQLite
- Action types supported in V1: `LaunchApp`, `OpenUrl`, `FocusWindow`,
  `RunSystemCommand`. (No conditionals, no loops, no inputs.)
- Surfacing: Scenes appear in search by name (e.g. "Start Work")
- Authoring: a dedicated Settings panel for create/edit/delete + a
  command-palette `scene save <name>` that captures current open
  windows and apps as a Scene
- Cross-platform: works on Mac/Win identically; Linux best-effort
  (Wayland window focus may not chain reliably)

Why this is the right first flagship: visceral demo, uses
already-shipped action types, validates the Action registry by
composing it.

### Phase 2b — MCP host (second flagship, target: 8-12 weeks after Phase 1)

- New result kind: `Ask` (or `>` route reuse) opens a chat surface
- MCP client connects to local stdio MCP servers from a config file
- `@server.tool` mentions in chat let Claude call tools
- BYO Anthropic/OpenAI API key — no SaaS dependency
- Watson ships a `@watson` MCP server that exposes our own actions
  (LaunchApp, FocusWindow, etc.) as tools, dogfooding the design

### Phase 2c — Extension API (target: after MCP lands)

- TypeScript modules export `Resource[]` and `Action[]`
- Manifest format with permissions (network, fs, etc.)
- Git-based registry — `watson install github:user/ext-foo`
- No marketplace, no paid extensions in V1; community-driven

### Phase 3 — Differentiate

- Snippets-with-variables (`{clipboard}`, `{date}`, `{selection}`,
  `{input:Prompt}`)
- Capture model rollup (notes + clipboard + snippets unified)
- Cloud sync (settings, snippets, scenes, extension list); self-
  hostable S3-compatible blob target
- AI commands as MCP tools (Summarize, Translate, Ask)

## Decision filter

When evaluating a feature proposal:

1. **Does it move us toward the target architecture?** If it adds a
   bespoke `SearchResult` block, extends `SearchAction`, or grows the
   panel-boolean count, push back hard. Wait for or do the refactor
   first.
2. **Does it work on Mac AND Windows?** If it's Mac-only or
   Windows-only by design (not by current implementation gap), it
   probably shouldn't be in the core — it's an extension candidate.
3. **Does it depend on us running a server?** If yes, push back. We
   don't run servers.
4. **Is it on the floor that Spotlight already gives away?** If yes,
   we can ship it but it doesn't move the needle competitively. Spend
   accordingly.

## Process notes

- **CHANGELOG entries describe direction**, not just diffs. "Snippet
  paste reliability" not "fix typo in keystroke handler."
- **Cuts ship alongside features.** Every release that adds something
  should ideally remove or merge something — even one cut per release
  signals consolidation over cramming.
- **`DESIGN.md` is updated when direction changes**, not when leaf
  features ship.

---

## Provenance

This direction was set after a paired analysis in May 2026:

- **Structural audit** of the codebase: cataloged 13 features across
  4 jobs-to-be-done, identified the god-function dispatcher, the
  closed `SearchAction` enum, the five mutually-exclusive panel
  booleans, and the missing Resource/Action/Workflow shape.
- **Competitive landscape**: tracked Raycast (MCP-native, Pro-tier
  AI, ~2,000 TS extensions); Alfred 5 (workflows + Universal Actions,
  no AI/MCP); Flow Launcher (OSS Windows, Ollama plugin); Spotlight
  in macOS 26 Tahoe (clipboard + snippets + app intents now free at
  the OS layer).

The conclusion was that Watson sits in a defensible-but-untaken seat:
**cross-platform + OSS + automation-native + MCP-host**. The
architecture has to catch up before that seat can be claimed.
