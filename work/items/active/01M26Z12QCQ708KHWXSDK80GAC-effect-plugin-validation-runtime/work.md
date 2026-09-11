---
id: 01M26Z12QCQ708KHWXSDK80GAC
title: Effect plugin validation runtime
status: active
created: 2026-09-11
source: direct
---

# Effect plugin validation runtime

Continue choice A from the approved plugin-standard work. Effect authors and
validates the nested RetroArch policy. Production, tests and previews use the
same shipped checker. Generated-schema replacement is not the chosen direction.

The research landed in `b6fd6bf3`. Its successful experiment is evidence, not
production runtime support. The user approved implementation under `plan.md`
on 2026-09-11. This includes the reviewed source-only runtime, common APIs,
completion rules and explicit public `effect/Schema` import. On 2026-09-11 the
user approved in-process preparation without hard preparation time or memory
limits. The decision below supersedes that part of the original plan. Source
registration still needs grounding in the real package producer. New authority,
ungrounded schema or broader execution limits still require a decision.
VM, device and deployment operations remain separately gated.

Authority: `docs/briefs/2026-09-09-plugin-authoring-standard-brief.md`, the user's
choice A, and `docs/research/retroarch-effect-quickjs-probe/TIMERS.md`.

## Design review

Consistency, feasibility, security and adversarial reviewers inspected the full
draft and source evidence. Nine P2 document issues were corrected. A follow-up
review confirmed those corrections and found no new P1/P2 contradiction.
The configured document-review model was unavailable; the same roles were
reviewed with the available reviewer model instead. No runtime tests or device
operations ran during planning. Production-boundary approval is now recorded
above. Source-registration extraction and target verification remain explicit
gates. Hard preparation containment is deferred by the later user decision.

## First implementation slice

`6bb27152` implements bounded source snapshots for the current single-file
producer. Administrator evaluation and approval use the same retained bytes;
installed registration no longer performs its own unbounded source read.
Compiled Android sources use the same in-memory interface. No dependency imports,
platform globals, new persisted schema or execution-budget change are enabled.
An independent review found no introduced P1/P2 defect.

Thirteen source-contract tests passed on filesystem and tmpfs, with focused
registry/interpreter/administrator checks. Fresh parent verification passed all
13 source tests and four installed-launch tests; the built-payload test remains
explicitly ignored. The installed suite's stale duplicate-setting assertion was
reproduced and corrected separately to require one winning native assignment.

The full korrid test task and full administrator crate test suite then passed
on the build machine. Logs are retained as `proc_7474` and `proc_204f` in this
session's process records. No VM or device check was run for this slice.

## Preparation limitation and accepted risk

Source inspection and an independent review confirm that stock Oxc 0.142 exposes
neither preparation cancellation nor a recoverable whole-preparation allocation
ceiling. Its arena capacity is an observation/initial allocation, not a maximum.
QuickJS's interrupt and heap limit do not cover Oxc, resolver or Rust JSON work.
A timeout around a waiting thread would not stop the native worker.

The original plan made enforceable preparation bounds a release gate. The user
subsequently chose: run source preparation inside korrid and defer hard
preparation limits. After the stall/crash risk was stated explicitly, the user
said "make it so" on 2026-09-11.

This decision changes only the preparation guarantee. Keep bounded source
admission and the existing QuickJS execution, stack and output limits. Measure
preparation separately, but do not present measurements or checks between
parses as cancellation or a process memory ceiling. A pathological parse can
stall or crash korrid, including when the plugin source was approved.

No disposable helper or parser fork is authorized or needed for this chosen
approach. Do not build either as a prerequisite. Actual Effect validation,
source-only shipment, one checker in production/tests/previews and no
compatibility branches remain binding. Module loading and the required platform
APIs still need implementation. This decision does not mark them complete.
No VM, device, deployment or publication operation is authorized by this change.

## Closed modules and shared completion

The next slice implements in-process preparation of admitted relative ES-module
graphs. Declaration evaluation and launch use the same preparer and loader.
QuickJS owns cycles, live bindings and canonical module identity. Static missing
imports fail before execution. Dynamic imports, synthesized imports, package
imports and source escapes remain unavailable. Preparation uses retained bytes
only, with no filesystem callback. Existing source and generated-output limits
remain unchanged.

Every evaluator now uses one function-only timer scheduler and completion path.
It requires fulfilled module initialization, captures synchronous output before
queued mutation, drains initialization before launch, and rejects queued errors
or unhandled rejections before success. Jobs and timers share the existing
250 ms execution deadline. Timer-dependent top-level await remains unsupported.

A new regression exposed a QuickJS teardown abort when a queued callback retained
an unresolved module's promise resolver. The private timer queue now releases
its callback references through native array truncation while the context is
alive. Cleanup invokes no JS after interruption. Regression tests also cover
prototype tampering, rejected imported modules and a subsequent fresh runtime.

The Nix administrator source output previously copied `script.rs` without its
submodules. The real source-output check reproduced the missing `source.rs`.
Both Nix source producers now retain the exact shared Rust and scheduler source.
The administrator Nix package builds and passes its packaged tests.

Verification on the build machine: 15 module tests, 12 completion tests and all
13 source-admission tests pass. Focused cascade, native-setting, installed-launch,
registry, interpreter and administrator tests pass. Korrid all-target checking,
administrator strict all-target Clippy, Rust formatting and source-output byte
comparisons pass. Two independent reviewers found no blocking defect; their
additional lifetime and prototype cases are retained as passing regressions.
Process evidence: `proc_acc6`, `proc_0189` and `proc_d901`. No VM or target check
ran. The existing proseql dependency warning is unchanged.

This is not full Effect integration. Native npm/package-source registration,
CommonJS/JSON dependency loading, text/URL libraries, measured graph allowances
and the actual nested-policy connection remain unfinished. Do not present the
new timers or relative-module tests as proof that the production Effect checker
already runs.

## Execution ownership

The bounded snapshot slice is complete and landed. The broader Effect runtime
remains active and incomplete. Implementation resumes under the accepted
preparation risk above, not under a helper-process proposal.

The user clarified that this destination owns Effect work only. The source
session owns SSH rollout. Effect runtime work is not an SSH prerequisite. Do
not change publisher work, deploy to haku or change its services in this
session.
