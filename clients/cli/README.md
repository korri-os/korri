# Person CLI: `korri`

`korri` uses HTTP over the local person socket. It never opens korrid state,
loads signing keys, or connects directly to a peer. The CLI depends on korrid's
Rust library so `RpcRequest`, `RpcResponse`, and `RpcFailure` have one producer.
The cost is that a CLI build also compiles the daemon's library dependencies.

## Commands and checks

- `nix run .#korri -- --help`
- `nix run .#cli-check` — CLI format, clippy, and unit/real-process tests.
- `nix run .#cli-daemon-check` — focused socket, authorization, forwarding,
  sandbox, settings, and daemon-argument tests. This is not the full korrid gate.
- `nix build .#checks.x86_64-linux.korrid-linux-device-module --no-link`

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
one response. Setting writes return their final native response only. `version --json` and
`completion <shell> --json` return JSON strings. Help stays text, even when
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
writes also allow one exchange. Other requests keep the 30-second default.
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
and archive helpers remain. This cut does not implement the missing
plugin-prepare entry point (#223).

**Unresolved acceptance blocker:** safe offline `korri identity reset` is not
implemented. It returns `IdentityRecoveryUnavailable` without state access,
even with `--yes`. There is no native exclusive-maintenance contract to fence
all writers and socket reactivation or preserve service-owned files when reset
recreates them. Known stopped units alone are not proof. The functioning old
`korrid identity reset` remains hidden operator tooling until a safe replacement
or a reduced contract is approved. It has none of those new safety guarantees;
this documentation is not permission to run it.

Physical controller testing remains portal-only; `controllers seats` changes
the configured count. Setup does not duplicate the optional starter-pack offer;
existing effects remain through `plugins on` and `library folders rescan`.
System launch-setting inspection requires explicit game context; see
[the current wire limits](PLAY-LIBRARY.md#settings-and-current-producer-limits).
Richer terminal pickers, address/free-space facts and power RPCs are deferred.
Remote session/plugin administration is not widened by this CLI.

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

`target` resolves exact keys or case-insensitive names. Ambiguous labels always
stop, including on a terminal. PeerList supplies verified owner membership;
korrid checks signed membership and revocations again before forwarding.
Forwarding uses the existing signed NativeClient transport and unchanged peer
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

The peer policy table itself is unchanged. Host settings snapshots now work for
verified same-owner peers so rename can obtain the target's revision. Host peer
metadata advertises only `device-name`, not player count or credentials.

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

IdentityStatus now embeds the existing IdentityState, read from live authority
without opening or creating identity storage. Invalid identity diagnostics use
safe failure text. Typeshare's exact internal-tag mapping follows the existing
artifact-state mapping pattern. Regenerate it with
`nix run .#korrid-check -- --types-only`; never edit generated TypeScript.
