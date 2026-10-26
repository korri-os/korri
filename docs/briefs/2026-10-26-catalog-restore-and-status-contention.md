# Catalog restore and status contention — 2026-10-26

> **Correction:** e391's single-batch restore did not fix the fresh overlay →
> End offset on RG DS. The manual repair below was not proof of that combined
> transition. Catalog restoration is superseded by
> [the Chromium 153 two-phase settle](2026-10-26-chromium153-catalog-settle.md).
> The status-catalog mutex fix described below remains in force.

## Portal restoration

The multi-output catalog restore now sends these operations in one geometry
IPC batch, followed by exact portal focus:

```text
[con_id=N] fullscreen disable; [con_id=N] floating disable; [con_id=N] fullscreen enable global; [con_id=N] focus
```

Output power remains its existing separately measured batch. Catalog restore
no longer temporarily floats/resizes the portal. This branch is shared by all
`surface_displays` requests with no game placement and scope `all`: opening the
catalog from the overlay, post-End/stop/exit catalog presentation, and catalog
presentation after startup/recovery. There is no other portal global-fullscreen
command producer in korrid. Overlay and single-display placement stay unchanged.

### Evidence and confidence

- **Verified locally:** headless Sway 1.12, two 640×480 outputs, Chromium
  147.0.7727.116 and Chromium 155.0.8059.79 on Wayland. A page with fixed color
  bands exercised floating 640×480 at (0,480) and global fullscreen 640×960.
  The original +240 offset did **not** reproduce in three cycles per version.
  Both the old restore and the corrected restore rendered the bands correctly.
- **Verified locally:** the incomplete two-command repair (`floating disable;
  fullscreen enable global`) reproduces the wrong 640×480 portal at (0,480),
  still marked fullscreen mode 2. HEADLESS-1 becomes black. The three-command
  repair restores a tiling `con`, fullscreen mode 2, rect (0,0,640,960), with
  correctly aligned bands on both outputs. The corrected Chromium DOM viewport
  is 640×960. CDP inspection used no viewport emulation.
- **Verified by the parent on RG DS:** a fresh play → Leave → End reproduces
  the +240 offset. The same incomplete two-command repair gives the wrong
  single-output rectangle. The complete three-command repair restores attract
  content to its clean-boot position on both displays. Evidence files include
  `offset-before-DSI-2.png`, `offset-after-DSI-2.png`, and `variant-DSI-2.png` /
  `variant-DSI-1.png` under `/tmp/rgds-acceptance/`.

The exact Chromium protocol condition that makes the initial +240 offset
persistent on the device remains unproven. Returning to boot's tiling state is
verified by the parent's device experiment, not by pretending the offset also
reproduced with the local Chromium fixture.

Local captures, IPC snapshots, probe page/scripts, and results are under
`/tmp/korri-catalog-geometry/`. They are not deployment artifacts.

### Sway 1.12 source

Read `/nix/store/2j7grygxd5da5r214vzz3y8b54rr739k-source`:

- `sway/tree/view.c:1033–1062`, `view_center_and_clip_surface`, centers a view
  when it is **floating OR fullscreen**. The offset uses current content size
  minus `view->geometry`, divided by two. Disabling floating does not, by itself,
  prove that fullscreen centering stops. Clipping also depends on CSD state.
- `sway/desktop/transaction.c:237–251` applies current container state and calls
  that centering/clipping function, including after a configure timeout.
- `sway/desktop/xdg_shell.c:313–334` updates `view->geometry` from committed xdg
  geometry. Floating views additionally feed this geometry back into container
  size/configuration; tiling views do not take that floating-size path.
- `sway/ipc-json.c:606–607` serializes IPC `geometry` from `natural_width` and
  `natural_height`. The device's reported 640×480 `geometry` is **not proof**
  that committed xdg window geometry is stale.

Fake-Sway tests pin the complete restore batch and reject changing floating or
re-enabling global fullscreen while the portal is still fullscreen.

## Return contention

Parent-provided device transition, `korri resume` through the person CLI:

| Phase | ms |
|---|---:|
| Total | 997 |
| Mutex wait | 489 |
| Game placement/focus, including display disable | 269 |
| Reconcile | 104 |
| Unit change | 100 |
| Unit read | 32 |

The wait accounts for 49% of the total. Game focus is not the extra ~400 ms;
without the recorded mutex wait, the remaining transaction is about 508 ms.
The sampled `status_poll` holder name does not identify every holder that may
have acquired the non-fair mutex during the wait.

**Verified code/test cause:** browser/person status called
`describe_active_session` while `observe_session_status` still owned the native
transition mutex. That description loaded the complete catalog merely to find
the active title. `catalog_snapshot_blocking` walks static and dynamic games,
loads the dynamic source, and reads per-game person play-log statistics. Peer
label enrichment also ran in that protected callback.

A public-router regression gates catalog description indefinitely. Before the
fix, exact Return cannot complete until the gate opens: the deadlock-bound test
records a 2001 ms `status_poll` mutex wait. After the fix, exact Return completes
while catalog description is still blocked. Releasing the old status response
preserves its title and historical snapshot but cannot republish Home.

**Fix:** keep native liveness/focus, launch-scoped journal facts, pending
identities, and Home correlation inside the transition. Move catalog/title and
peer-label enrichment to a blocking presentation worker afterward. The worker
only enriches the captured response; it never reconciles or publishes intent.
Active and overlay copies get the same presentation metadata.

**Inferred device cause:** this explains a long `status_poll` hold, but the old
device journal does not time catalog loading separately, so it cannot prove
that all 489 ms came from that operation. The new build emits
`SessionCatalogDescription { launch_id, elapsed_ms }` for descriptions taking
at least 100 ms. Those descriptions no longer own the transition mutex.
Catalog loading may still delay the status response itself. This change does
not cache or redesign catalog data.

## Checks still required

Run device acceptance on the committed build: repeat play → Leave → End,
Return, crash/exit, and restart recovery. Check final tiling/global state and
both captures. Compare transition mutex wait and the new slow-catalog timing.
No RG DS access or deployment was performed by this child agent.
