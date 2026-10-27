# Session status contention — 2026-10-26

The old spanning-catalog investigation has been folded into
[main-display portal](2026-10-27-main-display-portal.md). Spanning restoration
is no longer product behavior. The status mutex fix below remains in force.

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
