# Implementation Plan

## Goal

Replace global phases with a rolling merge train that keeps agents busy, runs expensive validation once per integrated checkpoint, and lands the product and streaming-host changes as clean cuts with no backward compatibility.

## Findings

Phase 1 took almost a day because the work was parallel only at the branch level.

- The tickets were not single-context slices. Ticket 01 has 10 acceptance criteria across Nix composition, six product parts, one device migration, account ownership, a global product check, and integration-branch policy. Ticket 04 crosses korrid, inputd, the portal, two surfaces, session recovery, focus, controls, and UI notices. Ticket 05 adds a service, key custody, first-boot binding, Nix isolation, restart recovery, and a VM test.
- The resulting branches are large. The current diffs range from 21 to 51 files. The product-module branch changes 50 files and deletes 8,869 lines. The live-session branch adds 4,763 lines. The RG35XXSP branch adds 16,441 lines. One agent still had to understand and verify each large slice from end to end.
- The branches were not independent. Plugin lifecycle contains the admission commit. Product module, local signer, and live session all touch korrid or Linux host composition. Several branches change the same VM test and Nix modules.
- Every worktree repeated discovery, compilation, review, and broad validation. Six concurrent Rust and Nix jobs also compete for the same CPU, memory, disk, and Nix daemon.
- The global phase barrier left completed work unmerged. The repository rule says to land finished work promptly. All six worktrees still exist, and the live-session worktree has uncommitted changes.
- Coordination state is unreliable. Every Phase 1 worktree has the same stale `progress.md` text about unrelated discovery regressions. The published tickets still say `ready-for-agent` after the branches produced commits.
- The workers produced one large commit each. That made review and conflict resolution wait until the end instead of happening while other work continued.

The fastest safe method is a rolling dependency queue with two clean-cut integration branches, one integration owner, and one expensive-validation lane. Do not run another global phase.

## Tasks

1. **Close the current Phase 1 merge train before starting more broad work**
   - Files: Phase 1 branches and `.scratch/korri-product-build/issues/01-07-*.md`
   - Changes: Assign one integration owner. Rebase and review the existing branches in dependency order. Land plugin admission before plugin lifecycle because lifecycle already contains the admission commit. Rebase product module, local signer, live session, and RG35XXSP separately against the new tip. Preserve each clean public behavior. Reject unrelated expansion rather than carrying it into later work.
   - Changes: Commit or discard the uncommitted live-session changes before integration. Update each ticket status when its commit lands. Stop using the copied `progress.md` files as task state.
   - Verification: Run focused tests after each rebase. Run one combined foundation validation after all accepted Phase 1 commits are together. Do not run the full repository suite after every cherry-pick.
   - Acceptance: Main contains every accepted independent Phase 1 foundation. No finished branch waits for a later phase. The tracker matches Git.

2. **Replace the three-phase schedule with a rolling dependency queue**
   - File: `.scratch/korri-product-build/README.md`
   - Changes: Remove the rule that all tickets 01 through 14 must finish before tickets 15 through 18 start. A ticket starts as soon as its actual blockers have landed on its base branch.
   - Changes: Keep two named clean-cut branches only:
     - `integrate/product-module` owns tickets 01 and 08 through 11. Device migration commits stay off `main` until the product check passes for every exported device in one cut.
     - `integrate/streaming-host-plugin` owns the final ticket 15 cut. The new plugin and deletion of the shared-host composition land together. No compatibility unit, disabled legacy unit, fallback package, or dual composition reaches `main`.
   - Changes: Tickets 12, 13, 14, 16, and 17 run continuously from their direct dependency commits. Ticket 18 starts when its software and hardware blockers are ready. They do not wait for unrelated tickets.
   - Acceptance: The schedule has no global phase barrier. Only the two changes that require an atomic clean cut wait on integration branches.

3. **Use one integration owner and many narrow worker agents**
   - Files: No product file is owned by the scheduler. Each worker receives an explicit path set before launch.
   - Changes: The integration owner maintains the dependency queue, assigns bases, reviews commit boundaries, rebases finished work, and lands accepted commits. The integration owner does not implement product behavior.
   - Changes: A worker owns one externally visible behavior and one commit. If a ticket crosses independent ownership areas, split its execution into directory-owned commits under one ticket lead. The lead integrates those commits in its worktree before review.
   - Changes: Workers do not edit shared tracker files, `progress.md`, or common VM test files unless the assignment names those files. This removes avoidable conflicts.
   - Acceptance: Several workers can edit disjoint paths at the same time. One owner resolves ordering and prevents completed branches from waiting.

4. **Run review and validation as separate continuous lanes**
   - Files: Existing package tests, Nix checks, and the product VM test.
   - Changes: Each worker runs formatting and the smallest tests that prove its public behavior. A review agent reads the commit while the next workers continue.
   - Changes: One validation lane runs expensive checks on the newest integrated checkpoint. If a newer checkpoint supersedes a queued run, cancel the older run. Do not launch the same full Rust, portal, Nix, or VM suite in every worktree.
   - Changes: Use three verification levels:
     1. Worker level: focused unit or evaluation tests and formatter.
     2. Integration level: affected package suites plus product check on the combined commits.
     3. Cut level: full required checks once before a clean-cut branch lands.
   - Acceptance: Full suites run once per meaningful integrated state, not once per worker branch. Test failures identify the smallest responsible commit.

5. **Start all direct Phase 2 successors immediately after their own blocker lands**
   - Files: Device directories, input-seat code, Linux streaming client, signer and identity code.
   - Changes: From the product-module base, launch RG DS, R36T Max, RP Mini V2, and Odin migration workers at the same time. Give each worker its device directory. Give one integration worker exclusive ownership of shared workflow and `flake.nix` edits.
   - Changes: From the live-session commit, launch queued-input reset and Linux streamed-session workers without waiting for device migrations.
   - Changes: From the local-signer commit, launch the backup and identity-switch worker without waiting for device migrations.
   - Changes: Run Odin hardware checks only after its combined cut builds. Do not make the three other migration workers wait for Odin hardware.
   - Acceptance: Seven independent workers can run after the three relevant foundation commits land. The product-module branch receives device commits continuously and runs one combined product check at the end.

6. **Prefactor Phase 3 work without landing a dual implementation**
   - Files: Streaming plugin source, image seeding producer, plugin selections, and sleep policy.
   - Changes: After admission and the ticket 07 decision land, workers can prepare the streaming plugin package, native artifacts, and focused policy tests on `integrate/streaming-host-plugin`. Nothing activates on `main` until ticket 15 deletes the old composition in the same cut.
   - Changes: After plugin lifecycle lands, start the image-seeding producer before device migration finishes. Keep per-device selection data in device-owned commits so agents can work in parallel. Land the producer only when the product module has the required image hook.
   - Changes: Resolve the two ticket 17 user choices before launching the sleep worker. Do not let an implementation agent wait for a live decision.
   - Acceptance: Agents prepare blocked work on clean-cut branches, but `main` never contains two runtime paths or a compatibility path.

7. **Use manual validation only for behavior that software cannot prove**
   - Files: Hardware acceptance records only.
   - Changes: Keep these manual checks:
     - RG35XXSP panel, buttons, Wi-Fi, and audio during tickets 06 and 18.
     - Odin portal rendering, touch, controller input, local RPC, game return, and restart recovery after ticket 11 integrates.
     - One real Linux near/far play, leave, return, and end run for ticket 13.
     - Streaming pairing and encoder, CPU, and temperature records for ticket 15.
     - Final first-supported-image acceptance per device model.
   - Changes: Do not require manual checks for Nix declarations, account values, approval digest agreement, removal recovery, identity journal recovery, or state-machine behavior. Tests and the VM own those behaviors.
   - Acceptance: Hardware access never blocks unrelated software work. Every manual check maps to a sensor, display, controller, network path, encoder, or real device lifecycle that CI cannot observe.

8. **Apply the method to the remaining queue**
   - Files: `.scratch/korri-product-build/issues/08-18-*.md`
   - Changes: Use this rolling launch order:
     - Product-module cut lane: 08, 09, 10, and 11 in parallel after 01. Integrate and land when the product check passes with no exception.
     - Session lane: 12 and 13 in parallel after 04.
     - Identity lane: 14 after 05.
     - Streaming cut lane: preparation after 02 and 07, final ticket 15 cut after the product-module cut.
     - Image lane: producer work after 03, device selections after the product-module cut, then integrate ticket 16.
     - Power lane: ticket 17 after 04 and the two user choices. Its product-module edit rebases onto the clean cut.
     - RG35XXSP lane: hardware work continues from 06. Ticket 18 rebases onto the clean product module and lands when its hardware facts are observed.
   - Acceptance: There are no numbered execution phases. The dependency queue stays full until only hardware-gated work remains.

## Files to Modify

- `.scratch/korri-product-build/README.md` - replace the global phase model with the rolling queue and clean-cut branch rules.
- `.scratch/korri-product-build/issues/01-18-*.md` - record actual status and remove the synthetic `Phase 2 gate` blocker where direct dependencies suffice.
- Existing product files - only through the ticket worktrees that own them. The scheduling change itself does not modify product behavior.

## New Files

- None required. The local Markdown tracker already holds the queue and blocking edges.

## Dependencies

- Task 1 must finish first because the current Phase 1 commits are the bases for all remaining work.
- Task 2 defines the branch and blocker policy used by Tasks 5, 6, and 8.
- Tasks 3 and 4 apply to every later worker.
- The product-module clean cut requires tickets 08 through 11 together, but tickets 12 through 14 do not depend on that cut.
- The streaming-host clean cut requires tickets 02, 07, the accepted lifecycle behavior from 03, and the product-module cut.
- Ticket 18 requires the RG35XXSP hardware findings from 06 and the product-module cut.

## Risks

- The current Phase 1 branches are much larger than their ticket boundaries. Review can find bonus refactors or hidden behavior changes. Rework can be faster than integrating an unclear branch.
- Product module, local signer, and live session overlap in shared Nix and korrid files. The integration owner must choose an explicit landing order and rebase each branch once.
- Concurrent heavy Nix and Rust jobs can make every worker slower. Keep one expensive-validation lane and let workers run focused checks.
- A shared integration branch can drift if workers branch from old tips. The integration owner must publish each accepted base commit and require workers to rebase before handoff.
- Zero backward compatibility means partial clean-cut commits cannot land on `main`. Preparation can exist on integration branches, but aliases, fallback reads, dual writes, disabled legacy units, and dual runtime composition are forbidden.
- Hardware availability can delay support claims. It must not delay software tasks that have automated acceptance.
