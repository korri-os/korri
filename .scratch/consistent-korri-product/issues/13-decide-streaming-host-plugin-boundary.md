# Decide the streaming host plugin boundary

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 07

## Question

Hosting a stream is a bundled plugin, on by default in most devices' plugin selection and removable through the plugin lifecycle, not a product part ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#streaming-host)). Today Sunshine is a systemd service composed by [korri-linux-host](../../../services/inputd/nix/korri-linux-host.nix) with a certificate control socket, seat wiring, and per-device encoder builds ([rg353m rkmpp Sunshine](../../../nix/devices/rg353m/default.nix)). [nix/device-cache/README.md](../../../nix/device-cache/README.md) states that plugin publication does not make Sunshine into a plugin; that sentence is revised by the parent decision.

What does the streaming host plugin declare, what effects must korrid own for it (certificate provisioning, seat lease, encoder selection, firewall), and which devices select it by default? Recheck the plugin boundaries rule in [AGENTS.md](../../../AGENTS.md): native programs arrive prebuilt, system configuration stays in native format, `plugin.ts` stays thin, and a plugin performs no effects. Compare with the existing Moonlight plugin direction in [Choose the opinionated plugin selection](03-choose-default-plugins.md#streaming-client) and with the RP Mini V2 case that hand-disables the units today.

Resolve with Simon the plugin's contributions, korrid's effect ownership, the per-device default selection including which devices omit it and why, and what evidence proves a device with the plugin removed still meets the product. Do not invent a plugin declaration schema; extract it from the existing plugin host receipts and the plugin briefs.

## Comments

Source inspection at `8196a356`, 2026-09-20. No builds and no device tests were run.

### The streaming host today is five parts, not one unit

`services/inputd/nix/korri-linux-host.nix` composes it. Every device that imports that host gets it: `odin2portal`, `r36tmax`, `rg353m`, `rgds`, `rpminiv2`. RG35XXSP does not import the host.

- `sunshine.service` takes `User` and `Group` from the runtime account, or from the `korri-sunshine-input-seat` group when seats are on (`:1246`). It carries about fifteen environment variables for the Wayland display, the XDG runtime directory, PulseAudio, D-Bus, certificate control, and the seat sockets (`:1190-1232`). It declares `Sockets=korri-certificate-control.socket` (`:1264`), orders itself after the compositor, the certificate socket, the input-source guard, and `network-online.target` (`:1171-1189`), and receives capture and encoder through an immutable argv (`:1265-1267`).
- `korri-certificate-control.socket` is root-created with an exact `FileDescriptorName`, and an assertion holds its inode at root:korrid (`:934-953`, `:775-783`).
- `korri-input-seat-receiver.service` and the `korri-sunshine-input-seat` group support protected launch-scoped controller seats (`:961-1012`). Udev rules move `uinput` and `uhid` to a Korri group (`:1309-1312`).
- KMS capture needs `capSysAdmin` through the NixOS wrapper, `CAP_SYS_ADMIN` in the bounding set, `NoNewPrivileges` off, and `PrivatePIDs` off. An assertion requires the DRM compositor backend for it (`:808-809`, `:1271-1284`).
- About twenty assertions bind the exact approved `sunshine-korri` build: patch names including `0020-add-korrid-certificate-control.patch`, the patch-set hash, the base derivation, and the build profile (`:707-760`, `services/sunshine/approved-patches.nix`).

### The plugin host does not accept that shape

`services/korrid/plugin-host/README.md:3,145,151`: the host accepts zero or one named native service per plugin. The unit allowlist is `[Unit] Description` and `[Service] Type`, `ExecStart`, `ExecStartPre`, `ExecStopPost`, `User`, `CapabilityBoundingSet`, `DeviceAllow`, and `LoadCredential`. `User` accepts only literal `root`. `Type` is `notify` or `exec`. Capabilities are limited to `CAP_NET_ADMIN` and `CAP_NET_RAW`. The only device request is `/dev/net/tun rw`. Unknown directives are refused by name. Declared ports become rules in the host-owned `korri-plugins` chain (`:157`).

`plugins/ssh/` is the working example of the intended shape: a three-line `plugin.ts` that names `services = ["sshd"]`, a native `sshd.service`, and a `plugin.nix` that exports `packages`, `files`, `services.sshd`, and `ports`. `docs/briefs/2026-09-15-plugin-model/plugin-contract.ts:48` gives the declaration its type: `Service { id, unit: FileKey, ports?: { tcp?: number[]; udp?: number[] } }`.

The gap is therefore not a missing `plugin.ts`. A second unit, a socket unit, a group, environment delivery, unit ordering, `CAP_SYS_ADMIN`, and DRM, uinput and uhid device access are all outside what a plugin can declare or what the host will admit.

### korrid already owns the certificate effect

Sunshine patch `0020` consumes one root-created `SOCK_SEQPACKET` socket. It verifies the absolute path, root ownership, the exact narrow mode, peer credentials, and the Sunshine host UUID before it serves provision, revoke, or attest (`services/sunshine/README.md:143-155`). korrid performs the provisioning. This is already the shape the federation rule asks for: the declaration describes, korrid acts.

### Which devices could select it

| Device | Sunshine unit | Capture | Encoder | `openFirewall` | Recorded encoder evidence |
|---|---|---|---|---|---|
| RG353M | runs | `kms` | `rkmpp` | `false`, with six TCP and six UDP ports opened per interface | RKVENC hardware H.264. Software x264 used 170 to 180 percent CPU and reached 72 C at 640x480@30 |
| Odin | runs | `kms` | `software` | `false` | none |
| R36TMAX | runs | default `auto` | default `auto` | `false` | no H.264 encoder exists |
| RGDS | runs | default `auto` | default `auto` | `false` | none |
| RP Mini V2 | force-disabled | not applicable | not applicable | `false` | none |
| RG35XXSP | no host module | not applicable | not applicable | not applicable | none |

Sources: `nix/devices/rg353m/sunshine-host.nix:83-127`, `nix/devices/odin2portal/web-session.nix:52-57`, `nix/devices/r36tmax/portal.nix:68`, `nix/devices/rgds/portal.nix:92`, `nix/devices/rpminiv2/portal.nix:111-133`, `nix/devices/r36tmax/CODEC-PATH.md:102,109`.

Only RG353M opens stream ports. It opens 47984, 47989 and 48010 TCP with 5353, 47998, 47999, 48000, 48002 and 48010 UDP on `enu1` and `wlan0`, and keeps the administrative port 47990 closed. Every other device runs the Sunshine unit with no open ports, so nothing on the LAN can reach it. Ticket 07's phrase "on by default in most devices" does not describe the tree today.

A sixth consumer exists outside the device set. An x86 host imports `korri-linux-host` and selects `sunshine.encoder = "nvenc"` (`docs/acceptance/sunshine-korri-headless-real-consumer-2026-09-01.md:22`). It is not one of the six devices and ticket 07 does not cover it.

### Plugin host coverage is partial

Only RG353M, RGDS and RP Mini V2 import the plugin host module (`nix/devices/rg353m/plugin-host.nix`, `nix/devices/rgds/default.nix:8`, `nix/devices/rpminiv2/game-plugins.nix:39`). Odin, R36TMAX and RG35XXSP do not. Ticket 07 puts the plugin host in the product module, so that gap closes there, not here.

## Answer

Resolved 2026-09-20. All choices are Simon's, selected through `ask_user`.

### Which side moves

The plugin host widens what it admits in a native unit, until the streaming host can ship as a plugin with its real systemd unit. korrid does not take the privileged parts and hand them back as a grant.

Simon selected this over the korrid-mediated alternative, with the cost stated before the choice. The narrow allowlist is the plugin host's security boundary today, so this is a security change and not a packaging change. It needs its own implementation and verification. Neither this ticket nor ticket 07 authorizes the wider admission by itself.

### How far the admission widens

The approval binds the reach. Each plugin's approval names the exact units, directives, devices, and capabilities that this plugin requested. The host refuses anything the approval did not name. One plugin can ship more than one unit when its approval names them; the streaming host needs three today.

This extends the existing native-request behavior rather than adding a second mechanism. `services/korrid/plugin-host/README.md:151` already lets a native `User=root` request select a different policy, names that authority in the report before approval, and puts the policy version in the effective unit and the approval digest. Widened admission follows the same path: the native request asks, the report names the authority, and the administrator approves the exact build.

Approving the streaming host therefore grants `CAP_SYS_ADMIN` and device access to that exact build. It grants nothing to any other plugin.

Cost: approval reports grow, and the administrator carries more judgement at approval time. A longer report is harder to read, and an approval that is hard to read is easier to approve without reading.

### Default selection

The streaming host is in the default plugin selection of every device in the current list except R36TMAX. Simon selected this in his own words: "only disaled by default on R36TMAX. for the rest of the current device list its enabled by default."

R36TMAX omits it because mainline exposes no H.264 encoder for that variant (`nix/devices/r36tmax/CODEC-PATH.md:102,109`). That is a recorded limit under ticket 07, not a disabled unit. R36TMAX must not keep a hand-disabled service.

Three consequences follow, and none of them is settled by this line alone.

- RP Mini V2 stops force-disabling the Sunshine service and the certificate socket (`nix/devices/rpminiv2/portal.nix:125-133`). Its comment records a milestone, not a hardware limit. Ticket 07 already refused the hand-disable, so the plugin selection replaces it.
- RG35XXSP is in the list, and it has no host module today. Its adoption belongs to [Decide RG35XXSP product adoption](14-decide-rg35xxsp-product-adoption.md). Default selection here states intent for that device; it does not complete its adoption.
- Odin selects the software encoder today, and RGDS selects `auto`. Selecting the plugin by default on those devices ships software encoding until each device proves a hardware encoder. RG353M measured software x264 at 170 to 180 percent CPU and 72 C at 640x480@30. Expect heat and CPU cost on any device that falls back to software.

### Port exposure

The plugin declares its TCP and UDP ports. The plugin host opens exactly those ports through its own `korri-plugins` chain, on every interface. Disable, removal, failed enable, and rollback withdraw the rules (`services/korrid/plugin-host/README.md:157`).

Sunshine's administrative port 47990 is not declared, so it stays closed. Open ports do not grant a stream. A client must still pair, and korrid owns certificate provisioning through the socket that Sunshine patch `0020` consumes.

Cost: RG353M loses its narrowing to `enu1` and `wlan0` (`nix/devices/rg353m/sunshine-host.nix:104-127`). Stream ports then also appear on its USB gadget link and on any interface added later.

### Which Sunshine build a device gets

korrid stays agnostic to the plugins it hosts. Simon stated the constraint directly: korrid must not hold Sunshine specifics. korrid therefore performs no encoder match and reads no encoder fact.

One streaming plugin release for each Nix system carries every encoder that the architecture supports. Sunshine selects its encoder when it starts, through its existing automatic selection. The encoder option in `korri-linux-host.nix:634-644` already defaults to `auto`. The per-device values exist because each approved build profile carries a different encoder, not because a person chooses one.

Costs:

- Each device stores encoder support that it never runs. Image size was never measured, because [Measure minimal and opinionated image sizes](05-measure-image-size.md) was skipped. The storage cost is unknown.
- Each encoder profile is a separate approved package today, with its own patch set and build profile (`services/sunshine/approved-patches.nix`, assertions at `korri-linux-host.nix:707-760`). One combined build changes that provenance contract.
- The evaluation-time assertions that bind the exact approved package disappear with the product-module composition. Install-time approval of the exact build replaces them.
- Automatic selection can choose a working but slow encoder. Nothing in this decision proves which encoder each device selects. Each device must record that result during acceptance.

### Acceptance

Simon approved these requirements through `ask_user`. They are requirements, not results. No check below has run.

- The product check passes on a device with the streaming plugin removed. That device boots to the portal, accepts input, and completes an authenticated RPC call to its local korrid.
- Removal leaves no Sunshine unit, no certificate socket, no seat service, no group, and no udev rule. It reclaims the storage, as [Decide how bundled plugins remain removable](04-decide-bundled-plugin-lifecycle.md#answer) requires.
- The approval report names the widened authority before an administrator approves it. The host refuses any directive that the approval did not name, and refuses it by name.
- On a device with the plugin enabled, a client pairs and streams. Disabling withdraws the firewall rules. Removal withdraws them and stops the daemon.
- Each device records which encoder Sunshine selected at start, with the CPU load and the temperature. Correct automatic selection is not assumed.
- R36TMAX carries a recorded limit for its missing H.264 encoder, and no disabled unit.
- RP Mini V2 carries no hand-disabled Sunshine service and no hand-disabled certificate socket.

Cost: every device needs physical retesting after the cut. The widened admission is a change to the plugin host's security boundary and needs its own review and tests in `services/korrid/plugin-host`. This planning session ran no build, no test, and no device command.

### Unresolved

korrid must stay agnostic to plugins, and today it is not. `korri-certificate-control` is named for the streaming host, and korrid speaks the protocol that Sunshine patch `0020` implements (`services/sunshine/README.md:143-155`). Moving Sunshine into a plugin does not by itself remove that Sunshine-specific knowledge from korrid. This ticket did not settle how the certificate effect stays generic. Settle it before the cut is implemented.

The input-seat lease has the same shape. `korri-input-seat-receiver.service` and the `korri-sunshine-input-seat` group are named for Sunshine and composed by the shared host (`korri-linux-host.nix:961-1012`). Whether the plugin ships them under its own approval, or korrid offers a generic seat, is undecided.
