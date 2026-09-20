# Decide what happens when leaving and returning to a game

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 01, 02

## Question

What must happen to an active game and its input when the user returns to Korri, returns to the game, invokes a plugin action, or ends the session?

The required controls are settled by [Define the intended Korri product](01-define-intended-product.md#live-game-controls). This ticket specifies their observable behavior, not whether they belong. In particular, the baseline does not decide whether opening Korri automatically pauses a game. Distinguish keeping a process running, pausing it, and restoring saved state.

Inspect the actual [portal session effects](../../../clients/portal/src/surface/use-launchables.ts), [overlay composition](../../../clients/portal/src/main.tsx), [host session control](../../../services/korrid/src/host/control.rs), and supported plugin actions. The current overlay's in-memory controller is not evidence of live controls. Follow actual producers for input ownership and local versus streamed sessions; do not invent identical pause or focus behavior for every runner.

Resolve with Simon the local and streamed cases, unsupported actions, and visible failure behavior. Preserve the existing requirement that Korri owns sessions and cleanup. Ending a session must not silently act on a different launch. Define what evidence proves the behavior before the shared product-boundary ticket chooses integration ownership and acceptance checks.

Automatic restoration after shutdown and cross-device save synchronization are deferred by the product decision. Do not add either as a prerequisite. Do not implement a new overlay, protocol, or session engine in this decision ticket.
