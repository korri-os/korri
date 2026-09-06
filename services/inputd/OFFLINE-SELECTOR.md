# Explicit offline bundle selection

`korri-bundle-select` has a separate, root-only operation:

```text
korri-bundle-select offline-select <expected-current> <new-bundle> --acknowledge-exclusive-quiescence
```

Success prints exactly `active=<new-bundle>` and exits 0. Failure exits 1.
There is no service argument, service operation, activation, health check, or
fallback. Normal `switch` and `rollback` are unchanged and are not called.

## Caller contract

The acknowledgement means the caller holds its external exclusive operator
lock, has stopped every bundle consumer and importer, has gated every start
path, and excludes concurrent selector, deployment, and store-GC writers. Keep
both exact bundles rooted independently. Hold these conditions through selector
verification and any following generation activation. The command **does not
inspect locks, systemd, running processes, or network isolation**. The flag is an
attestation by the caller, not proof that these conditions hold.

For the isolated Zao cutover, the root supervisor owns these controls. This
command is not a deployment runner and adds no lock-file or deployment schema.
Do not run it against the live selector merely to test it.

## Filesystem contract

The root remains `/nix/var/nix/gcroots/korri-bundle`, as defined by the existing
selector. It must already be a root-owned directory with mode exactly `0711`.
The command does not create it or fix its permissions. All parent directories
must be nonsymlink, root-owned, and not writable by group or others, including
`/` itself. The held `/` descriptor is checked before opening its first child.
Unsafe ownership or permissions cause refusal, never automatic repair. Directory
operations use held descriptors opened without following symbolic links.

Both arguments must be different, exact direct `/nix/store` directory paths.
Aliases, relative paths, trailing slashes, and normalized alternatives are
rejected. The existing `korri_inputd::bundle::resolve_bundle` validates both
bundles: all four executable symlinks, InputPlumber data, and the exact profile
inside the InputPlumber store item. This command does not duplicate that
validation or claim those components implement a particular owner/runtime
contract. The supervisor must verify the intended immutable component paths.

The existing `active` must be a root-owned symlink with exactly one hard link
whose raw target equals `expected-current`. A multiply-linked symlink is refused
before staging, without looking up `previous`: replacing it would change the
shared inode's link count and ctime. An aliased selector needs explicit operator
repair while consumers remain gated. The command stages an exclusively created
symlink in the same directory, reopens the root, then compares the active target
**and inode** immediately before `renameat`. It syncs the directory, reopens it, and verifies
the selected target and staged inode. `previous` is never read or written.
Temporary creation tries at most 16 names; collisions are never removed. On a
pre-rename failure, cleanup removes only the held temporary inode if it still
has its original name. Interrupted runs can leave temporary links; they do not
become recovery instructions.

This is not a kernel compare-and-swap. A noncooperating root writer can race
between the last comparison and rename. The external lock and exclusive
quiescence are required, not optional hardening.

## Failure and recovery

A rename, sync, verification error, signal, or lost command result requires the
caller to keep consumers stopped and gates installed. Re-read selection under
the same exclusive controls. Do not infer that a failure left the old target:
rename can already have succeeded. Filesystem calls are not given a wall-clock
timeout; interruption does not turn an ambiguous result into a safe rollback.

Recovery is another explicit `offline-select` with the exact current bundle and
an independently reviewed recovery bundle. It never reads `previous` and never
starts prior software. Proving that the recovery generation is safe remains the
supervisor's job.

## Verification

`tests/offline-selector.nix` is a standalone `pkgs.testers.runNixOSTest` accepting
pinned `pkgs` and the built `inputdPackage`. Its public CLI tests use real Nix
store fixture bundles and the actual fixed selector path **inside the VM only**.
They cover selection, explicit reverse selection, refusals, collisions, syscall
failures, concurrent replacement, post-sync verification, absence of subprocess
or socket effects, and selection surviving a VM reboot. VM-only root ownership
and group/other-write tests restore `/` in `finally`. Hard-linked `active` tests
check refusal with and without a `previous` alias. Unchanged-entry assertions
include device/inode, ownership, mode, link count, mtime, ctime, and raw symlink
target; atime is excluded because test reads can change it. Early refusals must
also preserve `/` and selector-root metadata and must not stage a link. All
syscall traces follow threads and are retained in the VM result. They do not call
normal switch/rollback. Bundle unit tests exercise the shared public resolver
using real temporary files. No production service, device, or private state is a test
fixture. VM fixture executables are inert; these checks do not prove Zao
activation, startup gates, streaming, or physical input behavior.
