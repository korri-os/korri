# Native surface treaty: proposal (2026-10-31)

Status: **core accepted by the owner on 2026-10-31** (C1–C3, C6, C7), with
D1 to D6 decided as recorded under "Decisions". D1 and D2 follow the owner's
own answers, interpreted as written there. Nothing
here is built in product code. It turns the spikes of 2026-10-30 and 31 into
one contract that parallel surface conversions can follow.

Grounding:

- The current treaty: `contracts/surface/korri-surface.ts` (TypeScript, mounts
  into an `HTMLElement`).
- The spikes, all on `main`:
  - `clients/zone-native-spike/`: Zone compiled natively and measured on the
    RG35XX SP.
  - `clients/zone-native-spike/logic-bench/`: Zone's TypeScript `update` in
    QuickJS on the SP.
  - `clients/surface-shell-spike/`: the runtime-loaded packages, the live
    switch on the SP, and the `CONTRACT.md` they follow.
  - `clients/surface-shell-spike/verify/`: conformance checks and the costs of
    many surfaces and of large game lists.

Per AGENTS.md, the existing treaty is the baseline. This brief lists only the
changes, each with the reason the evidence gives. Everything not named here
carries over unchanged.

## What stays the same

- **Model and host.** Korri hands a surface a model (facts) and a host (what
  it may ask for), and nothing else crosses.
- **Facts.** The model carries only facts Korri produces. Absent data stays
  absent.
- **Commands are fire-and-forget.** Korri publishes the consequence as a new
  model.
- **Types.** Every `Surface*` model type keeps its fields and meanings:
  catalog, status, settings, network, onboarding, store, invites,
  presentation, and the rest. So does every host command: launch, actions,
  settings, identity, gameplay controls, Wi-Fi, store.
- **Ownership.** A surface owns every pixel. It imports only the treaty.

## Changes

### C1. A surface is a package of files, not a JavaScript module

| Today | Proposed |
| --- | --- |
| `KorriSurface.mount(container: HTMLElement, model, host)` | A folder: `main.slint` exports the screen; `logic.ts` (bundled to `logic.js`) exports pure `init`, `update` and `view`; plus fonts and art. It contains no Rust and no native code. |

Why: a fixed Rust shell loaded Zone and Pico this way and switched between
them live on the SP. A switch took 0.24–0.25 s to Pico and 0.53–0.57 s to Zone.
Packages hold source only, so they need no exact compiler match. A
native-library plugin would need one, and the owner ruled that out.

### C2. Commands become requests returned by `update`

| Today | Proposed |
| --- | --- |
| A surface calls `host.launchGame(id)` and the other methods | `update` returns `{ state, requests }`. Each `SurfaceHost` method becomes one request variant with the same arguments. The shell performs them. |

Why: Pico and Zone already work this way internally (`src/state/requests.ts`
in both). It keeps `update` pure, so the verifier can replay it
deterministically.

Exception: `gameActions(gameId)` returns data synchronously today. A request
cannot do that. **Decision D3.**

### C3. Input arrives as messages, and the surface owns focus

| Today | Proposed |
| --- | --- |
| The host moves DOM focus and clicks (`clients/portal/src/input/spatial-focus.ts`). Surfaces get only `back`, `options`, `menu` and `system`. | The shell delivers `up`, `down`, `left`, `right`, `confirm`, `back`, `options`, `menu` and `system` as messages. Focus is part of the surface's model. |

Why: a native screen has no DOM, so the host cannot see what is focusable.
Both packages already keep focus in their models. The shell keeps the
portal's repeat timing (first repeat after 400 ms, then every 100 ms). The
shell, not the surface, still decides how each device maps to these actions.
The rule "the surface never learns which device produced an action" holds.

### C4. The game list is windowed

| Today | Proposed |
| --- | --- |
| `SurfaceCatalog.Ready.games` holds every game. | The surface receives the total and a window of games around a position it asks for. |

Why, from measurements on the workstation shell:

| Case | Result |
| --- | --- |
| Pico, 10,000 games | 1,254 MiB, 0–1.6 fps |
| Zone, 50,000 games | 1,029 MiB, 1–3 fps |
| Pico, 50,000 games | Ran out of memory at the 4 GiB cap |

Both surfaces project the whole list on every update while showing 5 to 12
games. The SP has 1 GB of RAM. Every surface author would otherwise have to
solve this alone. The shape is new schema, so **Decision D2.**

### C5. Images and heavy pixel work belong to the shell

The view hands images to the screen as `{ "$image": <url or path> }`. The shell
decodes each one once and caches it.

Pixel-by-pixel work in QuickJS is too slow. Pico's cover recolouring, a
64-pixel grid in 16 colours, took 1.4 s for 8 covers. Such work needs a
shell-provided operation that any surface can request. **Decision D4.**

### C6. Overlays come from the same package

The gameplay overlay already exists as `SurfacePresentation`
(`kind: "gameplay-overlay"`). The package provides it as a second exported
component in `main.slint` (or its own file), so switching surfaces switches
the overlay too.

Notices drawn over a running game (#179) need a second, see-through
layer-shell window from the shell. That is shell work, and no treaty change
beyond naming the component. **Decision D5** decides whether it belongs in
this contract now.

### C7. Packages are verified once at install; startup reads summaries

The verifier works as follows (`clients/surface-shell-spike/verify/`):

- It compiles the screen and lists its properties and their types.
- It runs `logic.js` through every action and 500 fixed pseudo-random presses,
  under hard QuickJS memory and time limits.
- It checks every `view()` against the screen's properties.
- It writes a derived summary: properties, result, slowest call, peak memory.
- 15 tests pass, including one planted defect per rule.

Costs on the SP:

| Work | 100 surfaces | 5,000 surfaces |
| --- | ---: | ---: |
| Full BLAKE3 hash | 1.2 s | 526 s |
| Read summaries | 0.03 s | 7.0 s |
| Full verification | 98 s | about 80 min (extrapolated) |

So verification runs once, at install, next to plugin-host's existing
signature check. Startup reads summaries and never hashes, because the signed
Nix store path already identifies the package.

The summary file's fields are spike-only. Making them product schema is
**Decision D6.**

The same QuickJS memory and time limits also apply at run time. A broken
surface then fails alone: the shell keeps the current surface.

### C8. A surface stores its own small preferences through a request

Today Pico keeps `pico.font` and Zone keeps `zone.look` in browser local
storage. A native shell has no local storage, so a surface needs a request to
store a small value under its own id, and receives it back in `init`. This is
new schema, grounded in those two existing producers. **Decision D1.**

## Known shell work (not treaty changes)

| Work | Evidence |
| --- | --- |
| Input through inputd and korrid, as the portal gets it, instead of reading `/dev/input` as root | The spike stopped InputPlumber to read the pad |
| Cache still parts of a screen | Zone uses about 90 % of a core on the SP. A compiled build that reused the still page used about 48 %. |
| Free fonts when a surface is dropped | About 104 KiB grows per switch (Slint 1.18.1) |
| Combined summary index instead of N files | Reading 5,000 separate summaries took 7 s at startup |
| Layer-shell window for notices over a running game | #179 |

## Decisions (owner, 2026-10-31)

- **D1. Preferences must be able to travel across devices.** Owner: "Sometimes
  the preferences must travel across devices."
  - Interpretation: a surface preference follows ADR 0018's rule for launch
    settings. Each preference kind is marked either shared (it follows the
    person) or device-only. The mark belongs to the kind.
  - Shared preferences are records (`CONTEXT.md`, "Record"), stored by korrid
    and copied between the person's devices.
  - The package declares each preference kind and its mark next to its logic.
    The verifier copies the declaration into the summary.
  - Record sync is not built yet (ADR 0018). Until it is, shared preferences
    are stored on the device and marked shared, so they travel once sync
    lands.
  - The declaration's exact form is still schema to draft and approve.
- **D2. The game list uses a cursor.** Owner: "Can we use a cursor approach?"
  - The model carries the total, one window of games, and opaque cursors
    before and after it.
  - A surface asks for games after or before a cursor, or from the start or
    the end, with a count.
  - Cursors stay valid when games are added elsewhere in the list. Index
    windows do not.
  - Consequence: anything a surface does over the whole list moves into
    Korri's query. Today that is Zone's FIND filter (`state/navigation.ts`,
    the band filtered through the find draft) and any sort or section split.
    Filter, sort and section become parameters of the cursor request, and
    the result arrives as a new window.
  - Jumping far, such as "first game starting with M", is a seek by key. It
    is a parameter of the same request, not an index.
  - Field names are still schema to draft and approve.
- **D3. Actions for the focused game.** The surface tells Korri which game is
  focused; Korri adds that game's actions to the model. `gameActions(gameId)`
  goes away.
- **D4. Image operations.** At first only Pico's palette recolour exists as a
  shell operation. Others are added when a real surface needs them.
- **D5. Overlays.** Only the gameplay overlay is in this contract. Notices
  over a running game (#179) come later, as shell work.
- **D6. The verifier's summary stays derived.** The shell rebuilds it when it
  is missing. It is not stored as plugin-host schema yet.
