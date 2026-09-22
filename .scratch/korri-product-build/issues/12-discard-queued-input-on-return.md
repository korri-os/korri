# Discard queued input on return

Status: in-progress
Blocked by: 04

## What to build

Make a returning game see a neutral controller before it receives live input. Buttons pressed or held while Korri is in front must not fire in the game.

## Acceptance criteria

- [ ] The input-seat path has a reset operation grounded in the existing seat protocol and live-session ownership.
- [ ] Return resets the exact live session's controller state after Korri stops owning the foreground and before live input reaches the game.
- [ ] Input received while the gameplay overlay or portal is in front is discarded rather than replayed.
- [ ] A held button produces a neutral state first and only produces a new action after a post-return live input transition.
- [ ] A stale launch cannot reset or affect the seat for a newer launch.
- [ ] Reset failure is visible, does not retarget another launch, and does not silently deliver queued input.
- [ ] Focused input-seat and korrid session tests cover queued presses, held controls, neutral state, stale identity, and reset failure.
- [ ] The change adds no hardware key codes or controller-specific behavior to the surface contract.
