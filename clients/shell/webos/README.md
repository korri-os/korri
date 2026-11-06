# webOS M2: native shell over the real local korrid

Workstation tasks never contact the TV. The parent owns all TV operations.
M2 lists the owner's federated library. It does **not** implement playback,
remote input, a webOS network-settings adapter, or a product device-join UI.
`launch.sh --fixture` retains M1's twelve sample games as an explicit fallback.
There is no automatic fallback to sample data on a real-brain error.

## Composition

`services/korrid/src/bin/webos-korrid.rs` calls the existing
`korrid::start_local_server`. That API selects an empty plugin registry and
supplies its own loopback ephemeral port and random per-process capability.
There are no local runners, launchers, installed plugins, systemd units, local
person signer, D-Bus discovery worker, iroh listener, or native stream viewer.
The core startup API and identity/federation schemas are unchanged.

The launcher starts this binary before the shell, using the existing shell
credential contract. It creates a fresh 0700 `${TMPDIR:-/tmp}/korri-shell.XXXXXXXX`
and `credentials/KORRID_RPC_CAPABILITY` is created once, 0600 and caller-owned.
Only the public `KORRID_ADDRESS` and `KORRID_SHELL_ORIGIN` go through stdout and
into the shell environment. The capability is not an argument, environment
value, URL, package entry or log. Both programs refuse root; core dumps are off.
The brain handles TERM/INT/HUP. Launcher exit kills and reaps both children and
removes only that launch's temporary directory, never the persistent state.

AppManager launches `org.korri.shell` through `launch.sh`. The launcher ignores
its optional launch JSON. It sets `APPID=org.korri.shell` and the existing
1080p adapter scale `SLINT_SCALE_FACTOR=1.5`. The shell runs `--installed
--packages packages --cache cache --surfaces zone,pico --placement fullscreen`.
`launch.sh [--fixture] --surfaces pico,zone` changes only the surface order.
F2 or the existing L1+R1 chord switches surfaces; no remote mapping is added.
Stderr appends to `/tmp/korri-shell-launch.log` in the app jail.

## Persistent allocation: operator decision, 2026-11-06

The parent inspected the running `org.korri.shell` jail. `/media/developer` is
its persistent read-write ext4 mount, mmcblk0p55. `/media/internal` is merely a
directory in the disposable jail tree, **not** an internal-storage mount.
HOME is the installed, root-owned app directory. It is not writable. `/tmp`
is disposable. The parent explicitly chose this allocation:

| Path inside the app jail | Purpose |
| --- | --- |
| `/media/developer/org.korri.shell` | App-created 0700 directory, app UID owner |
| `…/private` | Existing `KORRID_PRIVATE_STATE_ROOT` equivalent, 0700 |
| `…/storage` | Existing `KORRI_LOCAL_STORAGE_ROOT` equivalent, 0700 |
| `…/private/identity/device.key` | Existing device key, 0600 (`identity.rs`) |
| `…/private/identity/owner.event.json` | Existing signed public owner binding, 0600 |
| `…/private/federation/peers.json` | Existing peer memory, 0600 (`federation/store.rs`), once saved |
| `…/storage/device.yaml` | Existing readable device configuration (`config/snapshot.rs`) |
| `…/storage/upstreams.json` | Optional existing static peer configuration (`upstreams.rs`) |

The edge creates only the three allocated directories. It rejects symlinks in
all ancestors and rejects wrong type, owner or mode on those directories. It
does not repair, migrate, search alternate roots or use HOME. A nonblocking
exclusive directory `flock` covers the entire server lifetime and each offline
identity command. There is no lock filename or stale PID format. Run offline
commands only while the app is stopped, as the same app UID, in the same jail.
Do not run a second identity writer against this allocation.

**Known weakness:** `/media/developer` is root-owned 0777 without the sticky
bit. Another app can rename this allocation. 0700 protects file contents, not
the directory's name in its parent. This edge does not claim protection from a
hostile app replacing a parent during pathname operations. UID 6236 stayed
stable across the parent's uninstall/reinstall test; future UID changes must
fail closed, not create a different identity silently.

Parent TV checks, not yet performed by these workstation tasks:

1. Check `/media/developer` really is that read-write persistent mount.
2. Check all three directories are real directories, 0700, owned by the current
   app UID (observed 6236). Check no ancestor is a symlink. Check identity and
   federation subdirectories are 0700 and their files 0600.
3. Record `webos-korrid identity status`'s public device key. Stop/restart the
   app; reboot the TV; uninstall/reinstall the app. After **each** operation,
   check the same key and binding remain. Do not copy or log private keys.
4. Check only loopback TCP is listening for this binary. Check its actual RSS,
   idle CPU, startup time, library load time, and clean shutdown.

Uninstall the app before each IPK installation: webOS upgrades leave removed
payload files behind. The parent previously found an obsolete bundled SDL
being loaded from `lib/backports` after an upgrade. This package contains no
`lib/`, firmware, libc or SDL library. The persistent allocation is deliberately
outside `apps/usr/palm/applications/org.korri.shell`, which uninstall removes.
Never alter `org.korri.slint-spike` as part of this deployment.

## Joining: accepted development/operator path only

There is **no product TV join flow** today. No QR code, scan, or approval from
another Korri device exists at this edge. The parent explicitly accepted the
existing offline request/import contract for M2 development and testing.

With the app stopped, the parent runs these **inside its jail as the app UID**,
from the installed app directory. The transport into that execution context is
the parent's responsibility; running the command as SSH root is rejected.

```sh
./bin/webos-korrid identity status
./bin/webos-korrid identity owner-binding-request > /tmp/tv-owner-request.json
# Transfer this PUBLIC request to the person's signer. See the table below.
# The signer returns the complete signed public NIP-01 event as a file.
./bin/webos-korrid identity import --file /tmp/tv-owner-signed.json
# Or pass that same event on stdin:
# ./bin/webos-korrid identity import --stdin < /tmp/tv-owner-signed.json
./bin/webos-korrid identity status
```

These delegate directly to `identity_cli::run_from_environment`. They preserve
its bounds, kind 30078, tags, verification and idempotence. Import validates
that the signed **owned** event names the TV's device key. The operator must
check that the resulting `ownerPublicKey` equals the owner's existing federation
key. The offline import contract itself does not take an expected owner argument
or retain/check the request template; this edge adds neither. Transfer no person
private key and no other device's private-state directory to the TV.

### Who can sign that public request today?

The user's actual signer is unresolved. Do not confuse existing protocol code
with a shipped user command. The code/doc review found:

| Route | Existing command/action and limitation |
| --- | --- |
| TV request/import | Commands above produce and consume public data; **neither signs**. Linux's hidden `korrid identity owner-binding-request` / `korrid identity import --file PATH` have the same contract. |
| Linux active local signer | `LocalPersonSigner::sign` (`local_signer.rs`) rejects a binding whose device key differs from its configured expected device key. `korri-local-signer` is a socket-activated daemon, not a generic signing CLI. Do **not** repoint its credential to the TV. |
| Linux `korri identity switch nip46` | An existing command, but **not a join command**. `IdentitySwitchCoordinator::switch` requires a currently owned device and creates a replacement device identity, with data disposition and trust loss. It refuses the same owner. It cannot sign an arbitrary TV request. Do not use it for this operation. |
| NIP-46 signer | `Nip46PersonSigner` (`remote_signer.rs`) already supports `bunker://`, `connect` with `sign_event:30078`, then signing the exact template. It persists a separate client identity, checks the response author/template, and has a 120-second deadline. There is **no existing CLI command to submit an arbitrary TV template**. The user's signer application and an existing external NIP-46 client are needed; their exact commands cannot be stated until identified. The person approves the owner's same key in that signer, not on the TV. |
| Android NIP-55 signer | `Nip55PersonSigner.java` asks for `get_public_key`, then sends `ACTION_VIEW` with `nostrsigner:` plus the URI-encoded template, extras `type=sign_event`, `current_user=<owner key>`, `id=<request id>`. It receives the complete signed event via an Activity result or an already-authorized ContentResolver. This protocol can sign the TV template, but the Korri Android controller supplies its **own** device template and installs the result on that Android device. There is **no existing Korri command/export flow for a TV template**. A signer application's own arbitrary-event import/export UI or another existing client must be identified. Merely issuing `adb am start` does not capture its Activity result as a file. |
| Test signer | `org.korri.signer.test` is a physical proof signer, not the owner's key. Its signatures cannot join the owner's federation. |

`docs/research/korrid-identity-protocol.md` documents the lower-level signer
contracts. Its final claim that Linux NIP-46 orchestration is wholly absent is
older than the identity-switch coordinator: the coordinator now exists, but
still does not provide arbitrary-template signing or a new-TV join flow.
ADR 0031 keeps the request/import operations hidden operator tools.

**Remaining user action:** identify which signer holds the existing owner key,
and use its existing arbitrary-event signing route to approve exactly the TV
request and return the complete signed event. There is no honest universal
signing command in this repository. Do not claim joining is complete before
that event is imported and the owner key is checked. A user-facing TV join
mechanism remains an open product decision, not implemented here.

## Finding the other devices after binding

`start_local_server` runs `DiscoveryInputs::query_only`: it reads signed
same-owner roster and encrypted endpoint evidence, and publishes its owner
statement, but advertises no HTTP endpoint for the TV. The TV then fetches
catalogs directly using signed native peer RPC. Each peer supplies only its own
local games. Static configuration never substitutes for signed membership.

The parent must supply reachable peers and relay configuration:

- `KORRID_RELAYS` is the existing JSON-array environment override. The binary
  parses it with `RelayList::from_linux_environment`. It is public product
  configuration, not a secret. AppManager does not retain an ad-hoc SSH export.
- For persistence, use existing `host.relays` in
  `/media/developer/org.korri.shell/storage/device.yaml`. Grounding:
  `config/snapshot.rs::DEVICE_FILE_NAME`, `config/mod.rs::HostPayload`, and
  `relay.rs::RelayList::from_device_settings`. Preserve other document fields.
- The existing Linux product values are in
  `nix/product/requirements.nix::constants.relays`:
  `wss://offchain.pub`, `wss://nostr-pub.wellorder.net`,
  `wss://relay.primal.net`, `wss://nos.lol`, `wss://nostr.oxtr.dev`.
  `services/korrid/nixos-module.nix` exports the configured list as
  `KORRID_RELAYS`. Confirm the actual owner's other devices use that list or
  read their current `host.relays`; do not assume the TV can reach them.
- `storage/upstreams.json` or `KORRID_UPSTREAMS` can reuse the existing static
  native peer records (`upstreams.rs::UpstreamHostConfig`: `label`,
  `kind`, `baseUrl`, `devicePublicKey`, optional `moonlightAddress`). Copy actual
  **public** native peer records, not credentials or keys. The record must name
  the real device key. Restart after changing static configuration.
- This query-only edge has no LAN scan, mDNS, NetworkManager nearby links,
  Tailscale installation, iroh/STUN or federation relay listener. Same-LAN peer
  HTTP endpoints learned from relays work if reachable from the jail. Static
  endpoints work without relays. Internet-only peers that require hole punching
  will not be reachable by this M2 edge. ADRs 0026/0036/0041 describe the wider
  system, not capabilities this composition claims.

After import/configuration, launch the app normally. Zone should show peers'
real games once peer HTTP succeeds. Existing first-run setup still exists; skip
network changes (webOS owns networking) and finish setup in the existing UI.
NetworkManager-backed settings can fail on this TV; they are not a new webOS
configuration surface. No library or sample data is installed into real state.

## Build and package (workstation only)

Prepare the SDK as described in `../../slint-spike/WEBOS-CROSS.md` and apply
`../../slint-spike/WEBOS-RUNTIME-OVERLAY.md`. Neither the SDK archive nor public
TV runtime libraries belong in Git or the IPK. A relocated SDK must be relocated
again if copied to a different path. Use a clean committed checkout:

```sh
source_sha=$(git rev-parse HEAD)
test "$(git rev-parse HEAD)" = "$source_sha"
nix run .#shell-webos-build
nix run .#shell-check
nix run .#korrid-check
nix run .#shell-webos-package -- "$PWD/.cache/webos-m2-ipk" --source-sha "$source_sha"
nix run .#shell-webos-package-check -- --check-ipk \
  "$PWD/.cache/webos-m2-ipk/org.korri.shell_0.0.0_arm.ipk"
```

The ARM cache is `.cache/shell-webos-target`. Build uses at most two jobs,
hydrates the existing pinned proseQL source, and records source SHA plus all
three binary hashes. Packaging rejects dirty/mismatched source or changed
binaries. Zone/Pico install verification runs on the workstation; the TV only
checks installed file stamps. Choose a fresh output directory for each IPK.

`nix run .#shell-webos-proof -- OUTPUT` remains an **M1 fixture** SDL/Xvfb
1920x1080 proof with Zone/F2/Pico/F2/Zone. It does not demonstrate federation.
The focused `webos-korrid` tests run the real embedded brain and typed HTTP
boundary, test empty local catalogs, request/import and persistent identity over
restarts, exclusive ownership, permissions, symlinks and credential creation.
No ARM executable is run by the workstation tasks.

## Compatibility and acceptance risks

Target: `armv7-unknown-linux-gnueabi`, EABI5 soft-float, glibc 2.19,
`/lib/ld-linux.so.3`. The final IPK check inspects all three archived ELFs: no
GLIBC requirement above 2.19 and no Nix store dynamic reference. Kernel CSPRNG
uses getrandom's `linux_raw` backend, not a missing libc wrapper or jailed
`/dev/random`. QuickJS bindgen uses actual target headers and softfp ABI.
System LG SDL 2.0.4 remains the renderer's runtime dependency; nothing replaces
TV libraries. Bundled surface fonts are unchanged.

Only the parent can prove ARM execution, DNS/TLS/time correctness and outbound
relay/peer reachability from the jail. Check the TV's clock before signing or
reading expiring endpoint evidence. The binary binds only ephemeral loopback
TCP. It has no WAN listener or relay service. CPU/RSS and startup costs are
unmeasured on the TV; the full brain includes QuickJS, OXC and cryptography even
though no plugins are installed. The shell is a second native process. A
workstation build or empty-library test is not proof of acceptable TV memory,
performance, owner-signing interoperability, or real-library acceptance.
