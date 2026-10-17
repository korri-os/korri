# Gamescope support is designed from main's cases, not legacy's schema

AGENTS.md makes legacy's schema the presumptive baseline for anything moved to
`main`. On 2026-10-17 the owner chose a deliberate exception for Gamescope:
its settings, names and structure come only from real cases on `main`. Legacy's
`@korri:gamescope` plugin was built around Steam, a launch companion, runtime
control and stream control. Most of that has no consumer on `main`, and moving
its policy whole would bring that coupling with it.

Legacy stays readable evidence for failure modes, such as focus, input,
cleanup and breaking configuration changes. Do not copy a legacy field, name
or module shape into the new design unless a case on `main` needs it.
