# Plugin standard implementation status

This status accompanies `2026-09-09-plugin-authoring-standard-brief.md`.
The brief records decisions, not completion. Verified local slices do not mean
the end-to-end rollout is complete. No physical-device deployment or GitHub
publication is implied by this document.

## Implemented slices

- Named ES-module exports, bounded callback execution, generated publisher
  identity and full-key signature verification for cold and cached packages.
- Native service artifacts, directive validation, host hardening, declared
  IPv4/IPv6 ports and lifecycle cleanup. Native references require exact
  manifest dependency pins. Native launcher kinds require callable launch
  exports without running the callback during admission.
- Approved installed game declarations, dependency-first recovery, corruption
  isolation, current/previous selections and dependency-safe offline rollback.
- Real RetroArch and mGBA packages, installed Linux registry, runtime-user
  callback execution and per-runtime savestate separation. Android retains
  its separate platform declarations.
- Backend route candidates, saved game/system choices, per-launcher cascade,
  explicit selected launch and durable request replay protection. Same-game
  resume runs before fresh route resolution. Preference writes return their
  own committed revisions while holding the write lock.
- Runtime choosers in Shift and Pico. Device choice remains separate. Users
  can launch once or save/clear game and system preferences. Missing choices
  stay stored. An explicit producer fact distinguishes installed runtimes from
  local `host.toml` commands. Launch acknowledgements survive failed status
  reads but cannot replace newer observed session lifecycles, including exits.
- RetroArch source-backed scalar-setting evidence and native serialization.
  The pinned parser verifies effective values, not just emitted text. Each key
  has one assignment with the intended precedence. Compact raw assignments are
  normalized; invalid values are not copied into diagnostics. This is not yet
  complete nested-policy support, as described below.
- Exact-commit batch lookup verifies revision evidence and signed candidate
  outputs, then prints inspection and the separate exact-path approval command.
  Lookup does not install or enable a plugin.
- Optional `@korri:ssh` on TCP 2222. The user approved explicit per-build root
  service authority and public-key-only login for existing accounts. After a
  compatible host update, normal plugin enable/disable needs no system switch.
  Native artifacts own configuration and per-device host-key preparation.
  Recovery SSH is not replaced. See `plugins/ssh/README.md` for costs and limits.

## Verification on 2026-09-10

| Check | Result |
|---|---|
| Korrid Rust suite and prescribed Typeshare regeneration | Passed. |
| Portal tests and typecheck | 385 tests passed. |
| Pico tests and typecheck | 343 tests passed. |
| Shift tests and typecheck | 59 tests passed. |
| Browser fixture checks | Both surfaces passed at five container sizes, including focus, cancellation and permissions. |
| Native RetroArch parser regressions | 10 tests and 119 assertions passed. |
| Real packaged SSH process tests | Key acceptance/refusal, PTY, identity persistence, unsafe state, partial binds and driver-stdin preservation passed. |
| Plugin-host strict all-target Clippy | Passed. |
| Combined native plugin-host VM | Passed. The build finished in 493 seconds; the test script ran for 478.13 seconds. |

The VM verifies initially disabled SSH, root and ordinary-account login,
refused approvals and keys, toggle/firewall behavior, enabled/disabled reboot
recovery, damaged-key startup and restoration cleanup, purge, recovery SSH
coexistence, and unchanged system generations. Existing dependency, signing,
no-build, rollback and Tailscale lifecycle gates also passed. It does not prove
physical ARM gameplay.

Three earlier attempts exposed test-fixture defects: missing stderr capture,
a public substituter on an isolated node, and SSH consuming the command
stream. The last caused the one-hour timeout. The input-consumption defect was
reproduced without a VM before rerunning. Automated SSH now uses `-n` and a
20-second total timeout. The untrusted-key test also now requires real signature
rejection instead of accepting a configuration syntax error.

Evidence remains in the execution environment:

- `/tmp/pi-processes-hVmpYv/proc_20ac-stderr.log` records the passing VM.
- `/tmp/pi-processes-hVmpYv/proc_43d5-stderr.log` records final portal checks.
- `/tmp/korri-finish-pico-check-328c9453.log` and
  `/tmp/korri-finish-shift-check-328c9453.log` record surface checks.
- `/tmp/korri-runtime-screenshots-328c9453-fixed/` contains corrected captures.
  The earlier capture script reset scrolling after focus; it no longer does.

## Effect runtime direction

The user requires Effect at the authoring level or validation level. Production,
tests and previews must use the same shipped checker. There are no users whose
old validation behavior must be preserved. Compatibility branches are not wanted.
The user approved investigating Effect runtime validation before replacing it.

That investigation succeeded in an unshipped x86_64 QuickJS experiment. With
real text/URL libraries and deadline-bounded timers available, the actual Effect
decoder and renderer passed 14 fixtures in three rounds across three fresh
runtimes on each of two preparation paths. The existing 16 MiB memory cap and
250 ms execution deadline sufficed. Policy initialization and validation
scheduled zero timers. Timer availability was needed by a dependency at import.

See `docs/research/retroarch-effect-quickjs-probe/TIMERS.md` for reproduction,
measurements and limits. Source preparation is additional work outside the
execution deadline. Production source loading and API support remain unbuilt;
text/timer conformance and device performance still need work. No production
sandbox limits or APIs changed, and no generated-schema replacement was chosen.

## Still unfinished

- Connect the Effect-based nested RetroArch policy decoder and renderer to
  installed-package runtime input. Source-backed scalar evidence works, but
  nested-policy ingress and its source-module delivery remain unfinished.
  Do not present the scalar output map as a replacement user policy schema.
- Production publisher core-lock cutover and publication. The external
  `korri-plugins` branch `feat/plugin-standard` contains `ad103dcc`. Its sibling
  `feat/plugin-standard-ssh-328c9453` stages SSH re-export, checks and docs.
  Development evaluation passed with a local Git core override, including ARM
  output evaluation. The actual lock still names the previous API. Do not
  publish it against that lock or commit a local-path override.
- Haku generation integration, explicit receipt/data cutover, deployment,
  Tailscale account login/connectivity and physical gameplay verification.
  Haku is now reachable as `root@192.168.1.239`, still named `rg353m` and
  running kernel 6.18.2. The plugin host is absent and effective Nix `max-jobs`
  is still 4. The read-only probe confirmed key-only recovery SSH on TCP 22.
  The first deployment must preserve that access and install the compatible
  host with its download-only policy. No device generation was changed.
- Local owner enrollment and graphical Linux plugin management. The SSH slice
  implements the administrator CLI, not those missing consumers. The parked
  owner-approved SSH/public-image acceptance item remains open.
- Public-image console policy remains separate. Current main disables network
  SSH but retains passwordless root console access. This plugin does not make
  that image satisfy the parked secure-owner-enrollment requirements.

Full strict korrid Clippy also retains previously identified warnings outside
this change. They were not hidden by unrelated refactoring.
