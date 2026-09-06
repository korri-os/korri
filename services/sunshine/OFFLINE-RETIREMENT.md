# Offline all-client retirement

`sunshine-retire-all-clients` is a Linux administrative executable. It retires
**all** Sunshine client pairings. It does not expose a runtime RPC, socket or
HTTP operation. No admin password is needed or changed.

## Producer contract

The state schema is Sunshine's existing `nvhttp::load_state` contract in patch
`0020`: `root.uniqueid`, `root.named_devices` (`name`, `uuid`, `cert`) and its
historical `root.devices[].certs` reader. The reader is shared with the daemon
through `korri_certificate_control::parse_client_state`. This extraction does
not change how the daemon imports certificates or generates imported UUIDs.

The executable links the actual patched `korri_certificate_control.cpp` and
`crypto.cpp`. It calls `build_state_document(existing, unchanged_uuid, {})` and
`persist_offline_retirement`. This producer-only administrative entry reuses
`temporary_file`, `write_all` and `sync_parent`. It has no activation callback,
previous-document read or restoration path. The online
`replace_state_and_activate` contract and its rollback tests are unchanged.
The executable does not implement another state serializer.
The existing serializer removes only `root.devices`, empties
`root.named_devices`, and retains the existing host UUID and all other JSON
values. State whitespace/key order can change. Server certificate/private-key
files, app library files and history files are never opened by the executable.
Credentials or unrelated history stored in the state document are retained.

## CLI

```
sunshine-retire-all-clients --exclusive-quiescence-confirmed \
  --state ABSOLUTE_STATE_PATH \
  --expect-host-uuid EXPECTED_PUBLIC_UUID \
  --expect-state-sha256 EXPECTED_LOWERCASE_SHA256
```

There are no default paths, discovery probes, inspection mode, credentials,
force option or automatic retry. Arguments are public evidence, not file
contents. The digest is SHA-256 of the exact current state bytes. Obtain it
through the approved private preflight without publishing the state. Recheck
both public expectations before each explicitly approved invocation.

Run directly with the existing state's UID **and GID**. Running as root against
another UID's state fails; the command never chowns files or reads a private
key. The executable disables core/process dumps before reading state. The
supervisor must clear extra groups when dropping credentials.

The flag is an acknowledgement, **not proof of quiescence**. Before invoking,
the supervisor must stop and gate every Sunshine instance, socket activator,
pairing/launch path, and administrative state writer, and hold its external
exclusive maintenance lock. Keep them excluded throughout this invocation and
inspection. A process-local lock or a flock not obeyed by Sunshine is not a
substitute. Hostile root or a concurrent same-UID writer is outside this
primitive's safety contract.

| Result | Supervisor action |
| --- | --- |
| Exit 0, `retired_clients=N state_sha256=HEX` | The producer's replacement and parent directory were synced; the path was reopened and exact candidate bytes, safe metadata and empty producer membership were verified. Keep isolation until the wider cutover is accepted. |
| Exit 2 | Validation refused before transaction entry. No state write was attempted. Correct the evidence or inspect unsupported state offline. |
| Exit 3, signal, timeout, missing/unparseable output | Keep every authority stopped. The result is not success. Reopen and inspect under the same exclusion; never restore an old pairing snapshot or restart old trust automatically. |

Counts include both named and historical certificate records, not distinct
certificate fingerprints. Repeating with fresh exact evidence retires zero
clients but still performs and verifies a durable empty replacement.

## Filesystem bounds and failure policy

The path must be absolute, shorter than `PATH_MAX`, with no empty, dot, dot-dot
or symlink component. Ancestors must be owned by root or the caller and not
group/other writable. Root-owned sticky ancestors (for example `/tmp` in tests)
are allowed; the immediate parent must be caller UID/GID-owned mode `0700`.
Ancestor set-ID bits and extended metadata/ACLs are rejected. The current state
must be a regular, single-link, caller UID/GID-owned mode `0600` file, 1 byte to
16 MiB, with no extended metadata. The parent inventory is bounded to 4096
entries. A matching `STATE_BASENAME.korri-*` residue blocks the operation; the
command never removes an earlier run's residue.

The executable rejects malformed JSON, duplicate keys, more than 64 levels of
nesting, missing/mismatched host UUID, malformed known client collections and
invalid certificates. It accepts the historical PropertyTree empty collection
representation `""`. It refuses floating-point and integer-overflow values:
the producer's binary64 serializer cannot prove preservation of arbitrary
unknown numeric content. This conservative bound can block a legitimate state;
inspect that case rather than loosening checks during the outage.

The command pins a checked directory descriptor as its working directory and
passes only the basename to the producer's pathname-based transaction. It
compares the original file identity, metadata and bytes again before writing.
The existing producer creates an exclusive `0600` same-directory temporary
file, syncs it, renames it, and syncs the parent. State ownership and mode stay
the same; the state inode changes. Other file inodes and bytes are untouched.

**The actual rename is the no-restoration boundary, not reported success.**
The offline persistence entry never writes an old document, renames a recovery
file, or removes the replacement target. A failure before rename leaves the
exact old target untouched. Once the rename takes effect, the candidate stays
in place even if rename reports an error, parent open/fsync/close fails, final
reopen/verification fails, output fails, or the process is interrupted. A
failure or signal does not permit restarting authority. The caller must keep
exclusion and inspect every uncertain result; it must not restore old trust.

A process death before rename can leave a private temporary file. A death
after rename leaves the exact candidate in the running filesystem. This does
not prove power-loss durability: an unsynced rename can still have an unknown
outcome after a crash or power loss. Giving up automatic rollback is deliberate;
external quiescence and inspection remain necessary even when fsync completed
but directory close or reporting failed.

## Build and verification

The Linux-host Nix composition exposes package/app
`sunshine-retire-all-clients` and check `sunshine-offline-retirement`.
`nix build .#sunshine-retire-all-clients` runs the tests before installation.
`nix run .#sunshine-retire-all-clients -- --help` prints the contract.

Nix's syscall filter denies `setxattr`, and its `/tmp` may have an unmapped
owner. The build tests use the caller-owned build directory and explicitly skip
only xattr creation. The `tests` output contains both compiled test executables.
Run `test-offline-retirement CLIENT_ONE_CRT CLIENT_TWO_CRT SERVER_CRT SERVER_KEY
CLI_EXECUTABLE SAFE_TEMP_PARENT` outside that filter to include real xattrs.
All four input files must be newly generated synthetic fixtures, never copied
production state or keys. `SAFE_TEMP_PARENT` may be `/tmp` outside the sandbox.

Tests use only private temporary directories and freshly generated synthetic
RSA certificates. They exercise the shared state reader, writer and durable
transaction, then reopen the file and build the real Sunshine
`crypto::cert_chain_t` to prove previously accepted certificates are denied.
Tests cover unrelated state and external credential/library/history
preservation; evidence, mode, links, xattrs, malformed-state and residue
refusals; pre-rename write/sync/rename and post-fsync temporary-close failures;
post-rename directory-open/sync failures, successful fsync followed by a
reported sync/close error, rename error after the actual rename, final reopen
and verification-read failures. Each post-rename fault requires exact candidate
bytes and rejection of both retired certificates. A shared syscall audit
requires one candidate write, one temporary file, one rename and no target
removal, including in killed children. Online activation/restoration fault
settings are never reached by offline retirement.

SIGKILL tests pause midway through a real temporary write, before file sync,
after rename, after parent fsync, during final reopen, and before/after output
flush. Both the installed CLI and its unchanged entry source linked into the
audited test run against `/dev/full` and a closed output pipe; they return exit
3 or die from SIGPIPE without restoring trust. The audited CLI also verifies
exit 3 for post-fsync directory-close and post-commit reopen failures. The test
executable alone contains linker interception; the installed CLI has no fault
switches. The existing online transaction/activation/recovery tests still run
unchanged.

These are filesystem/process-interruption tests, not a power-loss simulation,
a live Sunshine restart, an active TLS-session termination test, or production
acceptance. The supervisor must separately prove all existing processes and
sessions ended and verify the deployed daemon denies retired certificates
before opening ingress. This command deliberately trades unattended recovery
for fail-closed inspection and requires every client to pair again.
