---
status: accepted
---

# Work lands on affected tests, and a nightly run catches the rest

**Context, 2026-11-17.** A small change to korrid costs about 1 minute to
rebuild the binary, 3 to 4 minutes to rebuild its 48 test binaries, and about
4 minutes to run all 2,282 tests. Most of this time goes to agents, who
iterate many times per task. The owner wants agents to iterate fast, and wants
changes on a device quickly when he validates the product.

## Decision

- During a spike or a playtest, an agent may run no tests. It builds, deploys
  and looks. Tests run when the work lands.
- Landing runs only the affected tests, not the full suite. Inside korrid,
  "affected" means the tests of each changed module and the integration tests
  that cover that module. The selection maps paths to tests.
- The full suite runs every night on `main`. When it fails, CI files a GitHub
  issue labeled `bug` that names the failing tests, ready for an agent to fix.
- The target for a small korrid change is under 60 s from the edit to a
  relevant test result.

## Consequences

A change can break a module that its path map does not reach, and the break
can reach `main`. The nightly run finds it up to a day later. This is
accepted in exchange for the fast loop. A red nightly issue is real work, not
noise, so the nightly run must not contain tests that fail because the
machine is loaded.

## Considered options

- The full suite before every landing: rejected, because it keeps the 4-minute
  floor on every landing.
- Affected tests while working and the full suite at landing: rejected for
  the same reason, at a smaller cost.
