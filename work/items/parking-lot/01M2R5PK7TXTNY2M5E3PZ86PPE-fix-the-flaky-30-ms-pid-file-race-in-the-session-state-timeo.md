---
id: 01M2R5PK7TXTNY2M5E3PZ86PPE
slug: fix-the-flaky-30-ms-pid-file-race-in-the-session-state-timeo
title: Fix the flaky 30 ms pid-file race in the session_state timeout test
origin: parked
status: To Do
priority: medium
labels:
  - korrid
  - tests
  - flaky
created: 2026-09-17
source: se-work
context:
  cwd: korri
  branch: main
  commit: 99d76d07
  repo: korri
  invoked_by: user
---

# Fix the flaky 30 ms pid-file race in the session_state timeout test

## Why it matters

`host::session_state::tests::hanging_systemd_helper_is_killed_reaped_and_returns_tagged_timeout` fails about one run in five on a loaded machine. A flaky test in the main suite costs a verification cycle every time it fires: the failure is a bare `Os { code: 2, kind: NotFound }` in a file the change under test never touched, so it reads as a regression and has to be disproved by re-running. It also makes the suite unusable as a gate while it stays red at random.

## Acceptance Criteria

- [ ] The test passes 20 consecutive runs on a loaded machine, or the timeout is raised to a value that survives the load
- [ ] The pid-file wait is bounded and reports which side timed out, rather than failing with a bare NotFound

## Related

- `services/korrid/src/host/session_state.rs`
- `services/korrid/tests/runtime_resolve.rs`

## Notes

Measured at commit c8d6055b on 2026-09-17, in worktree .worktrees/feat/runtime-resolve.

Reproduction: `nix run .#korrid-test -- --lib hanging_systemd_helper_is_killed`, five times.

Result: 4 ok, 1 FAILED. The failure is at src/host/session_state.rs:2786, `fs::read_to_string(pid_file).unwrap()` with `Os { code: 2, kind: NotFound }`. The helper writes the pid file, and the backend kills it after `Duration::from_millis(30)`. If the helper has not written the file in 30 ms, the test fails. Run 2 of 5 failed; the successful runs finished in 0.04s and 0.26s, so the margin is small.

It also failed once inside a full `nix run .#korrid-test` run on the same machine, which is what makes it expensive: it turns a green suite red with no code cause, and the failure names a file the change under test never touched.

Also seen: the same full-suite run passed on a re-run with no change, and 659 tests passed. So the suite is green apart from this one test.

Not caused by the runtime.resolve change: neither the file nor its helpers are in that diff, and the test touched no plugin code.
