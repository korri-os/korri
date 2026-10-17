# Every launch runs inside Gamescope unless the person turns it off

On a device with Gamescope, a launch runs inside it unless the person's
settings turn it off for a system, runner or game. Runners cannot declare that
they must not be wrapped. Only the person's settings decide, so the rule stays
in one place that the person can see and change. If Gamescope cannot start,
Korri refuses the launch and reports why. It never starts the game unwrapped
instead.

## Consequences

A runner that breaks when nested fails at launch until the person turns
Gamescope off for it. That failure is visible and has one fix, which is better
than a silent difference in how the game runs. A runner-side declaration can be
added later if real runners keep needing the same opt-out.
