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
completion rules and explicit public `effect/Schema` import. Establish source
registration and enforceable preparation safeguards before proceeding. New
authority, ungrounded schema or broader limits still require a decision.
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
above. Source-registration extraction and measured preparation/target limits
remain explicit gates.

## First implementation slice

`0727559e` implements bounded source snapshots for the current single-file
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

## Preparation gate

Source inspection and an independent review confirm that stock Oxc 0.142 exposes
neither preparation cancellation nor a recoverable whole-preparation allocation
ceiling. Its arena capacity is an observation/initial allocation, not a maximum.
QuickJS's interrupt and heap limit do not cover Oxc, resolver or Rust JSON work.
A timeout around a waiting thread would not stop the native worker.

The approved plan currently requires in-process preparation with enforceable
bounds. That gate is not met. Cooperative dependency-level bounds or a revised
isolation boundary need a decision before graph preparation can be enabled.
A disposable resource-limited Rust helper is a proposed investigation only; it
has not been authorized or implemented. Android process/packaging support is
unverified. Effect, source-only shipment and the shared checker remain binding.
No VM, device, deployment or publication operation ran in this implementation.
