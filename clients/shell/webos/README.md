# webOS M2: native shell over the real local korrid

Workstation tasks never contact the TV. The parent owns all TV operations.
M2 lists the owner's federated library. The embedded M4 viewer integration is
implemented but not yet TV-accepted; see [M4 integration](VIEWER.md).
Remote play keys, webOS network settings and product device joining remain out
of scope.
`bin/korri-webos --fixture` retains M1's twelve sample games as an explicit fallback.
There is no automatic fallback to sample data on a real-brain error.

## Composition

`services/korrid/src/bin/webos-korrid.rs` calls the existing
`korrid::start_local_shell_server`. It selects an empty plugin registry and
binds one loopback ephemeral listener. It derives the canonical
`http://127.0.0.1:<port>` shell origin from that still-open listener before
composing routes. No bind/close/rebind or placeholder origin is used. Its
per-process capability is random. The existing `start_local_server` entrypoint
retains its explicit-origin signature and behavior for JNI.
There are no local runners, launchers, installed plugins, systemd units, local
person signer, D-Bus discovery worker or iroh listener. The webOS edge now
supplies the embedded exact-launch stream viewer.
The startup signature and identity/federation schemas are unchanged. Embedded
startup now wires the existing bounded authenticated root WebSocket and shared
SurfaceSlot, as the shell requires for `app.surface.watch` model pushes and
ACKs. The same existing capability and exact Origin protect it. The webOS edge supplies measured physical gamepad baselines and samples through
the existing native input source. See [NATIVE-INPUT.md](NATIVE-INPUT.md) for the
producer path, compatibility review, and physical acceptance steps. No controller
or input is fabricated. The default embedded composition retains empty input.
This transport composition also applies to the Android/JNI caller of the same
startup API; no JNI signature or owner-binding effect contract changes.

The launcher starts this binary before the shell, using the existing shell
credential contract. It creates a fresh 0700 `${TMPDIR:-/tmp}/korri-shell.XXXXXXXX`
and `credentials/KORRID_RPC_CAPABILITY` is created once, 0600 and caller-owned.
Only the public `KORRID_ADDRESS` and `KORRID_SHELL_ORIGIN` go through stdout and
into the shell environment. The capability is not an argument, environment
value, URL, package entry or log. Both programs refuse root; core dumps are off.
The brain handles TERM/INT/HUP. Launcher exit kills and reaps both children and
removes only that launch's temporary directory, never the persistent state.

AppManager launches `org.korri.shell` through `bin/korri-webos`. The launcher ignores
its optional launch JSON. It sets `APPID=org.korri.shell` and the existing
1080p adapter scale `SLINT_SCALE_FACTOR=1.5`. The shell runs `--installed
--packages packages --cache cache --surfaces zone,pico --placement fullscreen`.
`bin/korri-webos [--fixture] --surfaces pico,zone` changes only the surface order.
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

The edge creates only the three allocated directories. A nonblocking exclusive
directory `flock` covers the entire server lifetime and each offline operator
command. There is no lock filename or stale PID format. Run offline commands
only while the app is stopped, as the same app UID, in the same jail. Do not
run a second identity writer against this allocation.

**Developer-mode boot policy:** the parent verified that
`/etc/init/devmode.conf` runs `chmod -R 777 ${TMPDEVDIR} ${MEDIADEVDIR}` on every
boot while developer mode is enabled (`MEDIADEVDIR=/media/developer`). Before
server startup or any offline operator operation, the TV edge therefore audits
its entire allocation, then re-tightens its directories to 0700 and regular
files to 0600. It logs one stderr line naming `devmode.conf` only when it changes
modes. It does not change the shared parent, siblings, ownership or file content.

Any symlink, foreign UID, or entry other than a directory or regular file causes
refusal before any permission repair. Multiply-linked regular files are also
refused: changing one could affect a file outside the allocation. Traversal
uses directory-relative, no-follow descriptors, retained through the audit and
permission changes. Filesystem/resource errors fail closed. Credential directories
remain strict; this repair applies only to the persistent allocation. No HBC
hook, identity migration, alternate root or HOME fallback is installed.

**Known weaknesses:**

- Other developer-mode apps can read the device key between the boot-time
  chmod and the first app launch or offline operation. Re-tightening cannot
  undo that exposure or revoke a key already copied.
- Turning developer mode off causes `devmode.conf` to remove
  `/media/developer` at boot. This wipes the identity, owner binding and other
  persistent app state. A later fresh allocation has a different device key.
- `/media/developer` is root-owned 0777 without the sticky bit. Another app can
  rename the allocation. Private modes protect contents after re-tightening,
  not its name in the shared parent. This edge does not claim protection from
  hostile concurrent replacement of that parent. UID 6236 stayed stable across
  the parent's uninstall/reinstall test; future UID changes fail closed rather
  than silently taking ownership or creating a different identity.

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
being loaded from `lib/backports` after an upgrade. This package bundles only the ss4s core and NDL module in `lib/`. It contains
no vendor runtime, firmware, libc or SDL library. The persistent allocation is deliberately
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
# Explicit M2 dev/operator action: keep the identity; claim no new identity.
./bin/webos-korrid setup finish
```

The identity commands delegate directly to `identity_cli::run_from_environment`.
They preserve its bounds, kind 30078, tags, verification and idempotence. Import validates
that the signed **owned** event names the TV's device key. The operator must
check that the resulting `ownerPublicKey` equals the owner's existing federation
key. The offline import contract itself does not take an expected owner argument
or retain/check the request template; this edge adds neither. Transfer no person
private key and no other device's private-state directory to the TV.

### Hidden development-only workstation signer

The parent identified the owner's signer as Amber 6.6.9
(`com.greenart7c3.nostrsigner`) on a Galaxy Z Fold7. The new **hidden, dev-only**
`webos-owner-sign` operator binary reuses `Nip46PersonSigner`; it is not shipped
in the TV IPK or listed in product CLI help. It reads a `bunker://` URL from
private stdin. The existing adapter supports **only signer-initiated bunker**,
not client-initiated `nostrconnect://`, so this command cannot print a connect
URI for `adb am start`. The user must create a fresh bunker connection in Amber
with the same account as their existing Korri federation. Never export an nsec.

Exact request/sign/import sequence (the parent owns TV and phone operations):

1. On the stopped TV, as the app UID in its jail, run the request command above.
   Transfer `/tmp/tv-owner-request.json` (public) to the workstation.
2. Read the existing owner's public key from `korri identity show` on an already
   owned device. Confirm that this is the account the user selects in Amber.
3. The user creates/copies a fresh bunker URL in Amber. Save it in a private
   mode-0600 workstation file `amber-bunker.txt`, in a 0700 directory. Do not put
   the URL in command arguments, shell history, logs or chat. The token's own
   relay URLs are the NIP-46 relays; the adapter validates them and does not
   substitute the TV's discovery relays.
4. On the workstation (substitute the observed **public** owner key):

   ```sh
   umask 077
   nix run .#shell-webos-owner-sign -- \
     --template /path/to/tv-owner-request.json \
     --expected-owner "$expected_owner_public_key" \
     < /private/path/amber-bunker.txt > /path/to/tv-owner-signed.json
   ```

   The person approves the connection and the kind-30078 TV binding request in
   Amber on the phone. Each exchange uses the existing 120-second deadline.
   The parent must coordinate readiness before initiating that timed action.
   Use a new bunker URL after a failed/consumed connection, not a retry loop.
   stdout contains only the complete verified signed event; progress/errors
   go to stderr. The command checks the canonical template before contacting
   relays, checks exact event equality via the existing signer, then applies
   the same public owned-statement verifier that TV `import` uses. It also
   checks the expected owner key before printing. Invalid/wrong-owner results
   leave stdout empty. The separate disposable NIP-46 client key and existing
   connection record live only in a 0700 temporary workstation directory,
   removed on success, error or Ctrl-C. No person private key is received.
5. Transfer the resulting **public** event back to `/tmp/tv-owner-signed.json`
   in the TV jail. Run `./bin/webos-korrid identity import --file
   /tmp/tv-owner-signed.json`, then `identity status`, while the app is stopped.
   Check its `ownerPublicKey` against the existing federation. Run the hidden
   dev/operator command `./bin/webos-korrid setup finish` while the app is still
   stopped, then launch. This calls the existing `onboarding::finish` producer
   under the same state lock, using its existing marker. It is explicit, never
   a side effect of import, and does not claim that a controller was tested.

The signature is the user's action. These instructions do not claim that Amber
interoperability, approval or the real owner binding has already been tested.

### Existing signer routes reviewed before adding the operator edge

Do not confuse existing protocol code with a shipped user command. The
pre-M2 code/doc review found:

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

**Remaining user action:** create the Amber bunker connection and approve the
exact TV request using the hidden workstation command above. Do not claim
joining is complete before that event is imported and the owner key is checked.
A user-facing TV join mechanism remains an open product decision, not
implemented here.

## Finding the other devices after binding

`start_local_server` runs `DiscoveryInputs::query_only`: it reads signed
same-owner roster and encrypted endpoint evidence, and publishes its owner
statement, but advertises no HTTP endpoint for the TV. The TV then fetches
catalogs directly using signed native peer RPC. Each peer supplies only its own
local games. Static configuration never substitutes for signed membership.

The package supplies product relays; the parent must verify reachable peers:

- Packaging evaluates `nix/product/requirements.nix::constants.relays` (#315),
  exactly as the Linux product does. This constant does not force the `korri`
  argument (the webOS composition passes `null`). Nix generates the existing
  JSON-array format into `product-relays.json` in the installed app directory.
  There is no hand-maintained copy of the list or new configuration schema.
- `bin/korri-webos` reads that generated product file and exports `KORRID_RELAYS`
  only in the real `webos-korrid` subprocess. The binary uses the existing
  `RelayList::from_linux_environment` parser. The shell and explicit M1 fixture
  receive no relay environment, including inherited overrides. Missing product
  data fails startup; it never selects fixture data automatically.
- **Do not copy product relays into `host.relays` or `storage/device.yaml`.**
  Those are user data, not the product composition. App replacement updates
  the generated file without writing the persistent `/media/developer` state.
  The parent's owned device `haku` was verified to use the same product list.
  Relay DNS/TLS/reachability from the TV still requires a hardware check.
- `storage/upstreams.json` or `KORRID_UPSTREAMS` can reuse the existing static
  native peer records (`upstreams.rs::UpstreamHostConfig`: `label`,
  `kind`, `baseUrl`, `devicePublicKey`, `moonlightAddress`). The serde reader
  permits an absent Moonlight field, but `UpstreamRegistry::build` requires a
  nonempty `moonlightAddress` for secure static native records, even for a
  catalog-only read. Preserve the actual peer record. Copy actual
  **public** native peer records, not credentials or keys. The record must name
  the real device key. Restart after changing static configuration.
- This query-only edge has no LAN scan, mDNS, NetworkManager nearby links,
  Tailscale installation, iroh/STUN or federation relay listener. Same-LAN peer
  HTTP endpoints learned from relays work if reachable from the jail. Static
  endpoints work without relays. Internet-only peers that require hole punching
  will not be reachable by this M2 edge. ADRs 0026/0036/0041 describe the wider
  system, not capabilities this composition claims.

After import/configuration, launch the app normally. Zone should show peers'
real games once peer HTTP succeeds. Discovery polls every 60 seconds; the other
devices first need to learn the TV's owner binding and publish endpoint evidence
addressed to its new device key. Allow more than one discovery cycle on a first
join, then inspect actual peer status instead of assuming an empty library is
final.

**Controllers:** The TV-side korrid edge now reads physical gamepads from
`/dev/input` and feeds the existing native-input source and controller facts.
The shell does not open input devices or initialize SDL controllers. Zone's
Controllers step can receive measured presence and live-sample evidence.
Physical TV acceptance is still required; see [NATIVE-INPUT.md](NATIVE-INPUT.md).
The hidden `setup finish` tool remains an explicit operator override, not a
controller test and not a prerequisite for normal first-run controller input.

WebOS still owns networking. NetworkManager-backed settings can fail on this
TV; they are not a new webOS configuration surface. No library or sample data is installed into real state.

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
six ELF hashes. Packaging rejects dirty/mismatched source or changed
binaries. Zone/Pico install verification runs on the workstation; the TV only
checks installed file stamps. Choose a fresh output directory for each IPK. Set `CARGO_BUILD_JOBS=2` for all
build/proof/check tasks and run only one expensive command at a time.

`nix run .#shell-webos-real-proof -- OUTPUT` runs the shipped launcher with
real `webos-korrid` and the real desktop SDL shell. Bubblewrap supplies fresh
private state at the binary's unchanged `/media/developer` paths, an isolated
loopback network and no host D-Bus. The proof uses the existing explicit
`setup finish`, reaches Zone, switches Pico/Zone at 1920x1080, restarts, and
checks that the device identity is unchanged. It captures images and logs;
no TV, phone, public relay or live user data is used.

`nix run .#shell-webos-proof -- OUTPUT` remains an **M1 fixture** SDL/Xvfb
1920x1080 proof with Zone/F2/Pico/F2/Zone. It does not demonstrate federation.
The focused `webos-korrid` tests run the real embedded brain and typed HTTP
boundary, test empty local catalogs, request/import and persistent identity over
restarts, exclusive ownership, permissions, symlinks and credential creation.
The embedded watch regression tests use a real signed peer and TCP sockets:
Loading → Ready with its game, later query updates behind exact ACKs, rejection
of missing/wrong capabilities, and shutdown with a watch open. The existing
owner-binding-coordinator platform composition (used by JNI) is also exercised
on the workstation. Android itself is not cross-built or run by these tasks.
No ARM executable is run by the workstation tasks.

## Compatibility and acceptance risks

Target: `armv7-unknown-linux-gnueabi`, EABI5 soft-float, glibc 2.19,
`/lib/ld-linux.so.3`. The final IPK check inspects all six archived ELFs: no
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


## M4b: one executable path for AppManager and NDL

`appinfo.json` now names `bin/korri-webos`. No `launch.sh` ships. The native C
launcher ports the previous script's sequence, including refusing root, umask
077, core dumps off, private per-launch directories, append-only stderr log,
explicit fixture selection, product relays only in the real brain, credentials,
startup polling, shell exit status and TERM/HUP/INT cleanup. It ignores optional
AppManager JSON. `KORRI_INPUT_TRACE` and `KORRI_SHELL_SEE_THROUGH` still pass by
inheritance. It never creates or changes a persisted allocation.

C is the smallest integration: the existing bounded C media CLI links directly
into the multi-call executable with its `main` renamed. There is no Rust FFI,
new viewer process, IPC or second executable path. Normal mode never initializes
NDL. The ss4s shared core loads at process start, but loads the NDL module only
in media mode. The two library lookup paths are package-relative RPATHs:
`$ORIGIN/../lib` on the launcher and `$ORIGIN` on the core. No absolute build
path or system library override ships. The launcher does not set LD_LIBRARY_PATH
for its children. The SDK and ss4s pins and close fix come from the NDL spike
at `9c33d631c`; no common-c, HTTPS client, audio or streaming session is added.
This is a local decode test, not completed M4 playback.

The launcher and shared objects are prebuilt by Nix (`media.nix`); the SDK's
ARM output is never run on the workstation. The same multi-call entry point is
tested against ss4s's native reference sink, including bounded duration, fixture
validation, pacing, looping, signal cleanup and close-time frame reporting.
The normal-mode tests compile the same launcher for the desktop. The existing
`shell-webos-proof` uses it with the real fixture brain and SDL shell.

The IPK checker inspects **all seven** ELFs: four executables and three libraries.
It checks ARM EABI5 soft-float, GLIBC ≤ 2.19, GLIBCXX ≤ 3.4.19 and CXXABI ≤ 1.3.7.
It audits strong imports and direct NEEDED against the original TV evidence in
`~/.cache/research/korri-webos-m2/evidence/tvlibs/` and the checked runtime in
`~/.cache/research/korri-lg-b7-prep-20261029T175543Z/readiness/tv-runtime-glibc-2.19/`.
Weak UND symbols are optional. GLOBAL/WEAK zero-valued ABS exports fail.

**Real provider, 2026-11-08:** the original TV `libgcc_s.so.1` now participates
in the same symbol audit, SHA256
`645c0dfa0eca6d830abbc82bb11a8441176c6d33309ae9254df09efbebe33aa1`.
The temporary 13-import exception is removed. Every required import needs a
real provider. This library is evidence only; it is never bundled.

### SDK preparation in a fresh worktree

```sh
prep="$HOME/.cache/research/korri-lg-b7-prep-20261029T175543Z/readiness"
clients/slint-spike/webos-sdk.sh \
  "$prep/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz" "$PWD/.cache/webos-sdk"
mkdir -p .cache/webos-target
clients/slint-spike/webos-runtime-overlay.sh "$PWD/.cache/webos-sdk" \
  "$prep/tv-runtime-glibc-2.19" \
  "$prep/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz" "$PWD/.cache/webos-target"
```

### Parent's TV acceptance (no role edit)

First obtain the owner's approval for any app registration/install operation.
Do not change the LS2 role, screensaver, power settings or persistent data.
Use the normal app UID inside its jail, never SSH root. Keep the original
system SDL. Coordinate readiness before a timed observation.

1. After the approved clean install, launch `org.korri.shell` normally through
   AppManager. Check unchanged Zone/library, remote and pad navigation, surface
   switching, then normal close and restart. Check the launcher log and that
   no launcher/brain/shell child or per-launch directory remains after close.
   The installed role should naturally name `.../bin/korri-webos`; do not edit it.
2. Keep that normal shell running. Put the existing spike's 300-frame fixture
   in the jail's temporary storage (not persistent state). From the installed
   app directory, as the **same app UID**, with its existing app jail:

   ```sh
   APPID=org.korri.shell ./bin/korri-webos media-test 10 /tmp/test-1080p30.h264
   ```

   No role edit and no LD_LIBRARY_PATH override is required. Expect module
   load, accepted 1920x1080 H.264 open, `frames=300`, `close complete`, exit 0.
   A child may re-exec the same installed path without changing its identity.
   On TERM/HUP/INT, expect owning-thread cleanup and exit 128+signal.
   Verify resource release, unchanged shell PID and healthy drawing afterwards.
   Accepted frames are not proof of visible video: opaque Zone can hide the
   hardware plane. The existing see-through diagnostic remains opt-in; this
   change makes no visual-design or power-policy decision. Stop on resource
   conflict or uncertain cleanup. Do not disturb an unrelated media pipeline.

The fixture is built by the existing spike producer (`stream` in its Nix file);
it is not game content and is not installed in the production IPK. This IPK
includes ss4s's LGPLv3 notice and the close patch. Public distribution still
needs the source/relinking and license review described in the embedded-viewer
brief; a local test package is not publication approval.
