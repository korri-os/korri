# Person CLI: `korri`

Owner commands use HTTP over the local person socket. They never open korrid
state, load device signing keys, or connect directly to a peer. ADR 0046's
author commands build locally and manage the author's publisher key only.
The CLI depends on korrid's Rust library so `RpcRequest`, `RpcResponse`, and `RpcFailure` have one producer.
The cost is that a CLI build also compiles the daemon's library dependencies.

## Commands and checks

- `nix run .#korri -- --help`
- `nix run .#cli-check` — portal setting parity guard, CLI format, clippy, and unit/real-process tests.
- `nix run .#cli-daemon-check` — focused socket, authorization, forwarding,
  sandbox, settings, and daemon-argument tests. This is not the full korrid gate.
- `nix build .#checks.x86_64-linux.korrid-linux-device-module --no-link`

The default CLI/package tests use configured author executables and do not
initialize Nix state. Real Nix key-format and scaffold parse/evaluation/build/
admission proofs remain explicit integration tests. Run both outside the build
sandbox with writable Nix state and store access:

```sh
nix develop .#korrid --command cargo test --manifest-path clients/cli/Cargo.toml --test plugin_author -- --ignored
```

The product installs `korri` on PATH. `--version` includes the source identity
when the Nix build provides one. Help, version, and shell completion work with
no daemon. `--socket` or `KORRI_CLI_SOCKET` selects a local Unix socket only.
The native NixOS module publishes the configured path in that environment
variable; its default is the same `korrid-cli/cli.sock` path as the Rust client.

## Commands and output

Run `korri --help` or `korri <area> --help` for exact arguments. Product areas
are `devices`, `library`, `launch-settings`, `downloads`, `controllers`,
`plugins`, `wifi`, `saves`, `identity`, `setup` and `metadata`. Session actions
are `play`, `leave`, `resume` and `end`. Provider acquisition is `get`.
[Play and library details](PLAY-LIBRARY.md) include launch-setting constraints.
[Portal parity audit](PARITY.md) maps the actual portal settings and actions,
including remaining gaps and recorded exceptions. Download a selected route's
required artifacts with `downloads status --game GAME --runner RUNNER`, then
`downloads start --artifact ID` (repeat `--artifact` for more IDs). These are
declared artifacts, never provider job IDs. Start does not also launch a game.

```sh
korri status --json
korri library folders list
korri library folders add /absolute/path/to/games gba
korri library folders rescan
korri controllers seats 2
korri leave
korri resume
korri end --yes
```

Human-readable output is the default. `--json` emits unchanged native Rust
`RpcResponse` values, not a second CLI schema. Ordinary commands return one
response object. `status` and `devices list` return arrays. `play` always
returns an array, including failed preflights; after launch it contains the
save-check and launch responses. Setup orchestration returns an array of its
completed operations; setup restart/finish and an initial failed status return
one response. Sideload returns an array of observed native responses: a fresh run
normally returns `[PluginArchiveBegin, PluginArchiveInspect]` or
`[PluginArchiveBegin, PluginArchiveInstall]`. The begin response's
`outcome.payload.uploadId` is the resume ID. Successful chunk/finish
acknowledgements are omitted; a failed upload step is the final response.
A resumed `--upload ID` run returns only its observed final response in an
array; it does not invent a begin response to echo the ID the caller supplied.
Setting writes return their final native response only. `version --json` and
`completion <shell> --json` return JSON strings. The offline author commands
`plugins new`, `build` and `publish` also return JSON strings; they have no RPC
response. Help stays text, even when
`--json` is supplied; neither help nor these commands contact korrid.

JSON stdout contains no prompts or progress. Usage, transport and orchestration
errors use native `RpcFailure` JSON on stderr. RPC failures remain in stdout's
response. Exit codes are 0 success, 1 failure, 2 usage, 4 unreachable local
korrid. Embedded failed jobs/artifacts, partial game lists and failed Wi-Fi
connection attempts also cause failure. A historical failed Wi-Fi attempt in
an otherwise successful status or scan is not a failed observation.

Prompts require both stdin and stdout to be terminals; piped use needs complete
inputs and explicit destructive `--yes`. Secrets use private stdin or hidden
terminal input, never arguments. `--yes` does not approve plugin permissions,
choose identity data disposition, acknowledge trust loss or confirm a backup.
Plugin installation requires the exact current digest from `plugins review`.

### Identity name

`korri identity name [NAME]` reads or edits the person name on this device.
Omit `NAME` to show the actual `SettingsSnapshot.personName`; the CLI does not
substitute a device label or another name when it is absent. An explicit empty
argument (`korri identity name ""`) is passed unchanged to the existing producer,
not treated as a special clear command. Human output shows the name returned by
korrid, which can differ from the submitted text.

Edits require `person-name` in `editableSettingIds` and the owner's available
signer. The CLI sends one `SettingsUpdate` with the snapshot's exact revision;
conflicts and signer failures stop without retry. Invalid names longer than the
producer's 64-character limit fail before RPC. Names, including spaces, are not
trimmed. `--json` preserves the final existing response, and identity operations
remain local. This command changes no person key or device ownership.

### Remote signer waits

Identity switches wait for korrid's NIP-46 deadline instead of the ordinary
30-second HTTP deadline. The wait derives from `remote_signer::REQUEST_TIMEOUT`
(currently 120 seconds), plus 30 seconds per exchange for relay publication,
transport and storage. A local-backup switch allows one remote exchange for the
old owner's revocation (150 seconds). A NIP-46 switch allows three serial
exchanges: connect, new binding, old-owner revocation (450 seconds). Person-name
writes also allow one exchange. Other non-plugin requests keep the 30-second
default; plugin waits follow the root host contract below.
The cost is a longer wait for a stalled signer; Ctrl-C can still interrupt the
CLI. This changes no server deadline, permission or recovery behavior.

### Hidden daemon tools and unresolved recovery

`korrid` with no arguments remains the daemon. Unknown words and daemon help
exit 2 before state or listeners. Use `korri --help` for person commands.
`korrid catalog pair`, `unpair` and `rescan` are removed without aliases.

Boot still uses hidden `korrid identity status`, `owner-binding-request` and
`import`. Hidden operator tools retained until cutover is confirmed are
`catalog migrate`, `identity replace-test-owner-offline` and
`identity reconcile-stale-grants-offline`. Internal plugin-run, plugin-launch
and archive helpers remain. Hidden
`korrid plugin-prepare SOURCE INPUT_JSON COMPOSITION_JSON RELEASE_ID` prints
Core's existing `PreparedLaunch` JSON record after effect-free preparation and
composition. It performs no declared effects and never appears in `korri` help.
External plugin checks can pass that exact record to `korrid plugin-launch`
instead of rebuilding Core's private handoff format.

**Unresolved acceptance blocker:** safe offline `korri identity reset` is not
implemented. It returns `IdentityRecoveryUnavailable` without state access,
even with `--yes`. There is no native exclusive-maintenance contract to fence
all writers and socket reactivation or preserve service-owned files when reset
recreates them. Known stopped units alone are not proof. The functioning old
`korrid identity reset` remains hidden operator tooling until a safe replacement
or a reduced contract is approved. It has none of those new safety guarantees;
this documentation is not permission to run it.

The existing native writers are `korrid.service`, `korrid-identity.service` and
`korri-local-signer-device-credential.service` (its `identity status` helper can
create the device key). The signer also has its own service. Activation includes
`korrid-cli.socket`, `korrid-control.socket` and `korri-local-signer.socket`.
`nixos-module.nix` supplies their accounts and `KORRID_PRIVATE_STATE_ROOT`.
Stopping those units does not prevent another start. Runtime masks are below
NixOS's `/etc/systemd/system` units in systemd's search path. Even a mask that
blocks a named unit does not fence the shipped standalone `korrid identity`
writers running as the service account. `IDENTITY_STORAGE` is a process-local
mutex, not an interprocess fence. No reset is attempted on that evidence.

The negative VM test is `services/korrid/identity-reset-vm-test.nix`. It uses the
real module and binaries. Root and person-account attempts, with and without
`--yes`, preserve state hashes, owners and modes. The identity service starts
despite runtime masks, and the standalone service-account writer can create a
different device key. The test restores the original state and verifies the
original identity after restart. This is not successful-reset acceptance proof.
Blocking all writers, interrupted reset recovery, safe post-reset restart and
retirement of the hidden reset remain unresolved.

`DeviceIdentity::reset` removes the whole `identity` directory, including the
device key, owner binding, NIP-46 connection credentials, person-name record and
identity-local authorization evidence. It creates a new unowned device key.
It does not reset the separate local person signer or reconcile an existing
`identity-switch` journal. Boot refreshes the signer's device credential from
`identity status`; automatic owner binding may then use the local signer.
A pending switch journal can instead reject the replacement identity as
inconsistent. A configured `ownerBindingFile` still binds the old device key;
boot's import rejects it after a raw reset. A raw reset is therefore not a
substitute for the live identity switch's authorization and journal recovery.
No games, saves or unrelated person data should be deleted to work around these
limits.

Physical controller testing remains portal-only; `controllers seats` changes
the configured count. Setup does not duplicate the optional starter-pack offer;
existing effects remain through `plugins on` and `library folders rescan`.
System launch-setting inspection does not require game context; see
[the current wire limits](PLAY-LIBRARY.md#settings-and-current-producer-limits).
Address/free-space facts and power RPCs are deferred.
Same-owner remote play and plugin administration use the boundaries below.

## Add an area

1. Add `src/<area>.rs` with a `Module` implementation.
2. Declare the module and add its instance to `MODULES` in `src/lib.rs`.
   These are the only shared registration edits.
3. Use `Context::rpc` for a single request or `Context::collect` for an
   aggregate. These respect the resolved `--device`; the transport remains
   local. Do not create an HTTP client, store, or remote endpoint in an area.
4. Return `Output::one` for the final existing `RpcResponse`, or `Output::many`
   for an array of existing responses. Keep failures as existing Err outcomes.
   `CliError` is for usage, transport, and orchestration errors.
5. Use `prompt::confirm` for destruction and `prompt::read` for missing
   ordinary input. Areas with secrets must use explicit stdin or a terminal-
   hidden reader, never ordinary echoed prompts or arguments. `--yes` confirms
   destruction only. It must never stand in for reviewed plugin permissions.
6. Add real-process tests in `tests/` and run `nix run .#cli-check -- <filter>`.
   Use a real Unix HTTP server with typed, configured responses. Test the
   daemon handler separately when an area adds a new RPC effect.

`target` resolves exact keys or case-insensitive names. Ambiguous device names
and game titles show a numbered picker only when stdin and stdout are both
terminals, without `--json`. Enter a number or exact ID, `/text` to filter by
name or ID, `/` to reset the filter, or `q` to cancel. No default is selected;
an exact ID takes precedence even when it is numeric or `q`. Piped or JSON
use stops with all matches and requires an exact ID/key. PeerList supplies verified owner membership;
korrid checks signed membership and revocations again before forwarding.
Forwarding uses the existing signed NativeClient transport and the explicit peer
policy. Local-only methods still refuse forwarding and direct the person to
SSH or local access. `--device` selects the device performing an operation,
not a game's streaming route. Device rename takes one settings revision and
sends one update. It never retries a conflicting write automatically.

## Security boundary

The separate native socket accepts root and the configured product runtime
account through kernel peer credentials. Group membership alone is not enough.
It carries RPC and forwarding only. It does not mount inputd's private input
WebSocket or grant inputd authority. The service's private state stays owned by
the service account, not by the person account.

The product runtime account also runs games. Both the person socket and its
persistent parent are mandatory InaccessiblePaths in the shared game/hook/
modifier sandbox builder. The negative test uses the generated paths and
systemd's real inaccessible socket/directory in an isolated user/mount
namespace. It verifies that a reachable socket becomes unreachable and its
parent cannot be inspected. Exit 77 records a host without these namespace or
systemd capabilities; it is not a passing negative connection proof.

## Host settings permission delta

| Caller | Device name | Player count |
| --- | --- | --- |
| Full browser | Newly supported | Unchanged |
| Local CLI | Supported, Full-browser authority | Supported |
| LocalSessions browser | Still denied | Unchanged |
| ReadOnly browser | Still denied | Still denied |
| inputd LocalUnixControl | Still denied | Still denied |
| Verified same-owner peer | Newly supported | Still denied |
| Other, unknown, guest, or household peer | Still denied | Still denied |

Host settings snapshots work for verified same-owner peers so rename can obtain the target's revision. Host peer
metadata advertises only `device-name`, not player count or credentials.

## Remote play and plugin permission delta

`--device` selects the device performing an operation, not a streaming route.
The CLI sends every request to its own korrid. That korrid verifies current
same-owner membership and forwards using the existing signed native peer
transport. The receiving korrid verifies evidence and revocations independently.
Ambiguous names stop before forwarding when piped; unavailable or unauthorized
peers never fall back to this device.

### Play

Host game listing now uses the same installed-library producer as Brain game
listing, including its diagnostics. Catalog/runner/settings preflight,
`SelectedGameLaunch`, session status and leave/resume/end already have
sufficient peer authority. No session permission
row widens. `SavesCheck` remains local-only: remote play retains its refusal as
an existing native `SavesCheck` Err, then sends the selected launch. JSON returns
`[SavesCheck Err, SelectedGameLaunch outcome]`. Exit 1 reports the failed save
observation even when the launch was accepted. Human output names the selected
device and says both that the launch was accepted and that this device did not
confirm its saves. Do not interpret that result as a clean preflight or retry it
as though no launch happened.

For a runner that declares save locations, the selected device checks its
stored save history under the native launch transition lock. A known stored
split refuses launch before hooks or the launch unit start. A runner with no
declared save locations can launch because it cannot restore or capture that
save history. This does not pull remote saves or confirm an unreachable peer's
save state. Use local saves commands on the selected device to inspect or resolve
that state. Local play keeps its current blocking preflight unchanged.

Controls pin the observed launch ID before confirmation. The selected device
checks that exact ID and its immutable starting-device journal. Another owner
device cannot adopt the session, and peers cannot adopt a locally started
session. Resume still needs real compositor focus proof. Selected launch now
uses the existing durable security-mutation replay journal, as plugin mutations
and exact-session controls already do.

### Plugins

Plugin administration includes installed and repository listing, repository
add/remove, catalog, inspect, install-disabled, enable/disable, publisher
list/trust/revoke, and chunked archive staging/inspect/install. It exposes no
raw-cache install, update, source switch, remove/purge, restore, or target-side
package build. Publisher trust is per device, not shared with the author's
computer or the owner's other devices.

Run `plugins review` and `plugins install --approval DIGEST` with the same
`--device`. Both inspect the selected device's package and execution policy.
Install re-inspects there and requires that exact digest. Enable re-derives the
installed report and checks the stored approved digest there. A stale digest
cannot install or enable a plugin. `--yes` never supplies that approval. There
is no second manifest, approval store, or copied signing credential.

Plugin calls have no total response deadline, locally or through a
peer. The root host bounds catalog transfers and detects stalled archives;
remote install waits as long as the target's active download does. Connection
and candidate selection remain bounded. Other native calls keep their 5-second
limit, and other ordinary CLI calls keep 30 seconds. Ctrl-C stops the CLI wait,
not remote work already accepted by the target. Inspect the selected device's
plugin state before retrying an interrupted operation. Mutations are never
retried automatically after send.

The root plugin host retains its lock, publisher signatures, immutable package
and dependency checks, native-artifact policy, receipts and no-build-on-target
path. The security cost is explicit: compromise of a verified same-owner device
now permits these plugin administration calls on its peers, including activation
of previously approved native artifacts. Repository registration is not plugin
approval. Removing a repository retains installed plugins and their state.

| Caller | Selected launch/status/exact controls | Plugin admin operations | Saves check/choose/restore, identity, Wi-Fi, onboarding |
| --- | --- | --- | --- |
| Full browser / local person CLI | Unchanged | Unchanged | Unchanged, local only |
| LocalSessions / ReadOnly browser | Existing session/read permissions | Still installed/repository listing only | Existing local permissions |
| Verified same-owner peer | Existing authority; starter and exact ID required for controls | Newly supported; root host checks unchanged | Still denied |
| Guest / household peer | Existing scoped stream authority only; no selected launch widening | Still denied | Still denied |
| Unknown, other-owner or revoked peer | Still denied | Still denied | Still denied |
| inputd LocalUnixControl | Unchanged | Still denied | Unchanged |
| Trust reconciliation | Unchanged | Still denied | Unchanged |

### Publishers and review

```sh
korri plugins publishers --device DEVICE
korri plugins trust @author --key 'label:FULL_BASE64_PUBLIC_KEY' --device DEVICE
korri plugins revoke @author --device DEVICE --yes
korri plugins review https://example.com/catalog.json @author:hello v1 --device DEVICE
```

`publishers` shows namespace, fingerprint, bound or pinned state, and installed
plugins. Revoke confirms before removing a pin. Config-bound keys remain host
policy; the CLI cannot replace or revoke them. Revoked plugins can no longer
start; stop and cleanup stay available. Existing running units stop at the next
disable, remove or restore-all, not immediately on revoke.

Review shows Bound, Trusted, New, Key changed or Unsigned for the root and every
dependency, plus native warnings and the exact approval digest. Fingerprints
use the key label and first 16 base64 characters, grouped in fours. Unsigned
releases show the full disclosure each time. Changed keys show both fingerprints
and the key-change warning.

On a terminal, review offers explicit trust for New or Key changed keys and
reinspects after trust. `--yes` never answers that question. Piped and JSON
review never prompt or trust. `trust --key` accepts a **full Nix public key**,
not a fingerprint. For a fingerprint-only input, run review to trust the key it
shows, or obtain the full key from the author. `--json` preserves the wire
response, including publisher facts and refusals.

### Plugin authors (ADR 0046)

These are the exception to portal mirroring. Nix and `korri-publish` must be
installed on the author computer (#327 remains out of scope). New, build and
publish need no korrid, and reject `--device`. No build runs on the target.

```sh
korri plugins new hello --namespace @author
cd hello
korri plugins build
korri plugins publish ./repository --release v1 --url https://example.com/plugins/
korri plugins sideload . --device DEVICE --platform aarch64-linux
```

New refuses existing folders and invalid names. Without `--namespace`, it asks
on a terminal; scripts must supply it. It writes `plugin.ts` without runner or
system IDs or a launch handler, a `flake.nix` using Korri's per-system `mkPlugin`,
`plugin.nix` with a native coreutils program, and the CLI build's complete
`korrid.ts` contract. Imports use `./korrid`; the
project needs no Core checkout or package registry for TypeScript checking.
The Hello runner opens `.hello` files with coreutils `cat`. On a device, its
output goes to the game's journal, not to the screen. Replace `cat` with your
runner's program in `plugin.ts` and `plugin.nix`. A plugin without a runner can
delete `plugin.nix` and the flake's `plugin` line; that argument is optional.

Build prints each Nix output. It builds the plugin's platforms that match Nix's
`system` or `extra-platforms` on this computer. It does not discover remote Nix
builder capabilities. Publish builds the same outputs, then calls
`korri-publish repository` with the explicit release label and HTTPS base URL.
It prints the repository folder and fingerprint; it does not upload anything.
The folder contains the catalog, archives and latest mark.

The first publish or sideload creates a Nix secret key with
`nix key generate-secret` at
`$XDG_CONFIG_HOME/korri/publishers/<namespace>.secret` (or under `~/.config` when
XDG_CONFIG_HOME is unset). Keys must be private regular files owned by you,
mode 0600. Keep this key backed up. Replacing it changes the fingerprint and
requires each device owner to trust the new key. Secrets never appear in output.

Sideload signs one archive locally with `korri-publish archive`, then sends it
through the local korrid to the selected device: begin with SHA-256 and size,
sequential base64 chunks of at most 1 MiB raw, then finish. Terminal output shows
upload progress. Peers do not report a platform yet: supply `--platform` for
native packages. Only packages the publisher marks `any` may omit it or use
`--platform any`; this builds locally and still requires the publisher to verify
that the package is architecture-independent.

The target must already bind or pin the signing key. When inspection refuses
an unknown/changed author key, terminal sideload shows the local author's
fingerprint and asks explicitly to trust it, then reinspects the same upload.
`--yes` supplies neither trust nor permission approval. The target retains that
upload for this trust step; an idle upload expires after 10 minutes. A fresh run
can resend its retained signed bytes once if the upload expires during terminal
trust. A resumed `--upload` run never rebuilds or resends bytes; a missing or
expired upload exits 1 and says to run sideload again without `--upload`.
Other failures stop without retry; korrid owns staging cleanup.

After inspection, terminal sideload asks for the exact approval digest. Piped
or JSON sideload without `--approval` returns the begin response and inspection
and installs nothing. Human output shows the upload ID and an exact resume
command. Review the inspection, then reuse that ID on the same device:

```sh
korri plugins sideload . --device DEVICE --upload ID --approval DIGEST
```

Do not begin another upload for the approval step: korrid has one active staging
slot per device. `--upload ID` skips the build and upload and reinspects the
same bytes before accepting the exact digest. JSON output stays an array of
unchanged native RPC responses, with no CLI envelope or copied begin response.

A publisher-trust refusal also returns the begin response and its upload ID.
Scripts must run `plugins trust NAMESPACE --key FULL_PUBLIC_KEY` on the same
`--device`, then resume with `--upload ID` to obtain and review the inspection.
Resume once more with `--upload ID --approval DIGEST` to install it. Terminal
resumes can offer trust using the same plugin directory (`[dir]`, default `.`): Nix
resolves its already-built outputs, the generated manifest supplies the
namespace, and the existing local secret key supplies its public half. This
performs no build, archive publication, or key creation and stores no CLI
session. Without that project or key, sideload gives the explicit trust command
and tells the owner to resume the same upload after trust. `--yes` never trusts
a key or supplies the approval digest.

The host rechecks the digest. Install leaves the plugin off; enable it with
`plugins on ID`. Owner approval and the sandbox are unchanged.

### Host-only process proof

`cli::remote_tests` launches independent korrid runtime processes with real
person sockets and signed native peer RPC. Launch units use the existing
in-memory backend, so these tests never operate a physical device. The plugin
proof also starts a plugin-host process and uses its real credential-checked
admin handler, approval checks, receipts and enablement logic. Its configured
artifact reports and native effects use the host's existing test runtime; this
is not proof of a physical device, repository network or new publisher policy.
Run the focused process proof in `nix develop .#korrid`:

```sh
cargo build --manifest-path clients/cli/Cargo.toml --bin korri
export KORRI_REMOTE_CLI_BINARY="$CARGO_TARGET_DIR/debug/korri"
cargo test --manifest-path services/korrid/Cargo.toml --lib cli::remote_tests::remote_process
cargo test --manifest-path services/korrid/Cargo.toml --lib cli::remote_tests::remote_process_cli_play_keeps_save_refusal_and_resolves_target_library -- --ignored --exact
cargo test --manifest-path services/korrid/plugin-host/Cargo.toml --lib remote_admin_process --no-run --message-format=json > /tmp/plugin-host-test-build.jsonl
export KORRI_REMOTE_PLUGIN_HOST_TEST_BINARY="$(jq -r 'select(.reason == "compiler-artifact" and .target.kind == ["lib"] and .executable != null) | .executable' /tmp/plugin-host-test-build.jsonl)"
cargo test --manifest-path services/korrid/Cargo.toml --lib cli::remote_tests::remote_process_plugin_approval_and_admin_boundary -- --ignored --exact
cargo test --manifest-path services/korrid/Cargo.toml --lib cli::remote_tests::remote_process_slow_plugin_peer_outlasts_only_the_plugin_wait -- --ignored --exact
```

## Product wire cut

| Removed native RPC tag | Current native RPC tag |
| --- | --- |
| `app.discovery.snapshot` | `app.library.folders.snapshot` |
| `app.discovery.pair` | `app.library.folders.add` |
| `app.discovery.change` | `app.library.folders.change` |
| `app.discovery.unpair` | `app.library.folders.remove` |
| `app.discovery.rescan` | `app.library.folders.rescan` |
| `app.session.freeze` | `app.session.leave` |
| `app.session.thaw` | `app.session.resume` |
| `app.session.stop` | `app.session.end` |

Library snapshots use `folders`; change/remove requests use `folder`. The end
reply phase is `ended` or `pending`, including the shared cancellation reply.
There are no native aliases or fallback reads. Scripts must update with korrid.
`SessionFreezerState` still reports real native `frozen`/`running` state.
`GameSource`, `LaunchSource`, `SourceStatus` and `AttentionItem.source` are not
library-folder types: attention can also originate in a declared game's
account root. Persisted discovery configuration, attention decisions and launch
records keep their existing schema and IDs.

Aka's separate external Effect-RPC adapter still calls that producer's
`app.session.stop`. It is not an accepted native korrid tag or an alias; the
adapter remains until aka switches protocols (AGENTS.md).

## Current wire limits

Status preserves all seven RPC results, including partial failures. Peer Ready
is a directory observation, not proof of present network reachability. Address
and free-space facts have no current RPC producer. Address is shown as unknown;
free space is deferred. Remote status preserves local-only authorization
refusals as failed sections. Devices list itself is local because its current
identity is local-only.

Address/free-storage and power schemas remain unresolved, not merely unwired.
Legacy's `product/apps/portal/api/device/status.rpc.ts` produces battery/network
state, but no address or storage measurement. Current interface enumeration
(`services/korrid/src/federation/interfaces.rs`) supplies transport candidates;
acquisition's `space.rs` measures the filesystem of an open download folder.
Neither chooses a person's status address or storage volume. A real status case
must decide which interfaces/addresses and which mounted filesystem(s) to report.

Legacy's Pico `PowerMenu.tsx` and `Panic.tsx` are static pages, not action
contracts. Legacy inputd's `power-suspend` command is an input action, not a
shared portal RPC. Current portal settings and `contracts/surface/` provide no
power action IDs or call payloads. A real portal power action contract must
define the calls and what Restart Korri restarts. No power command or host/device
effect is added before that decision; any implementation needs VM systemd/logind
proof.

IdentityStatus now embeds the existing IdentityState, read from live authority
without opening or creating identity storage. Invalid identity diagnostics use
safe failure text. Typeshare's exact internal-tag mapping follows the existing
artifact-state mapping pattern. Regenerate it with
`nix run .#korrid-check -- --types-only`; never edit generated TypeScript.
