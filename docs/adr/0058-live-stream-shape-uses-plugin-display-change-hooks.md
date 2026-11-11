# Live stream shape uses plugin display-change hooks

**Owner decision, 2026-11-10. Implementation in progress.**

A running streamed game follows its shape owner's screen. A fold or unfold
changes the shape without restarting the game or reconnecting the stream.
The first connected viewer owns the shape. When it disconnects, ownership
passes to the longest-connected remaining viewer. With no viewers, there is
no shape owner. A new viewer then supplies the next shape.

Rotation alone does not change shape. Landscape is the default. Only the
shape owner can use the session's Portrait toggle. That choice lasts until
the session ends. A per-game portrait default belongs to the later cascading
settings work, issue #395.

## Decision

korrid controls the unwatched display and performs the effects. A modifier
supplies its display-change hook as a file in its package. The gamescope
modifier's hook changes gamescope's live output size. No gamescope property
or binary name belongs in korrid's core. The hook runs with the game's runtime
user and sandbox, following ADR 0029's execution boundary.

This is a new plugin contribution, not permission for a plugin to run effects
while its declaration is evaluated. Existing signature and permission checks
remain required. Unlike ADR 0029's user-authored before/after steps, this hook
belongs to the installed modifier and its exact launch selection.

The transaction changes the Sway output and confirms its size, invokes the
modifier hook, then updates the viewer's encode size through the existing
runtime-resolution control. A failure must not be reported as a successful
reshape. Restore the previous output shape and any applied modifier state,
then show one "Couldn't reshape" notice. An unsuccessful restoration remains
an explicit error; do not hide it as a completed rollback.

No runner opt-in is required. A game that cannot refit can retain bars inside
the new window. No compatibility path is added for old gamescope packages.
Every gamescope package shipped with this change carries the hook.

## Grounding and limits

- `PluginDisplay` supplies width and height today.
- `SessionPrepared.display` and `KORRI_LIVE_DISPLAY` describe the far size.
- `host/display_placement.rs` already changes and reads back unwatched modes.
- `host/hooks.rs` and `LaunchUnitBackend::run_hook` provide the existing
  runtime-user execution boundary.
- Android's `KorriStreamViewerState.connected` observes an actual connection.
  Selecting a route or registering a stream player is not proof of connection.
- Android retains a runtime-resolution adapter, but current Sunshine does not
  carry its server counterpart. Plugins commit `32dfe4415` removed that protocol
  and its patches on 2026-10-23. The earlier research read a stale checkout.
  Live encode resizing without reconnecting therefore needs new server work.
  The Sway/gamescope demo proves live game rendering, not live encode resizing.
  This prerequisite must be resolved before the full transaction is implemented.

Connected viewers, their authenticated identities, and reconnect generations
must be tracked per exact launch. A stale disconnect must not evict a newer
connection. A joiner cannot claim another viewer's identity or change its size.
The hook must address only its launch's nested display, never a global X root.

## Trade-offs

The display has one shape. Non-owner viewers can see bars. Full screen-sized
rendering increases GPU cost. Hook execution adds a runtime failure point.
Unlike hard-coding gamescope in korrid, a plugin hook preserves the boundary
at the cost of a new contract and coordinated package deployment.

## Outside this change

Watched physical displays are not resized. Different shapes for simultaneous
viewers and a per-game portrait default are outside this implementation.
