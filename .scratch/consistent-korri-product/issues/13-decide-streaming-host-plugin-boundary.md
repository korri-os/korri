# Decide the streaming host plugin boundary

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
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

Only RG353M is a configured streaming host: `kms` capture, `rkmpp` encoder, and per-interface firewall ports 47984, 47989 and 48010 TCP with 5353, 47998, 47999, 48000, 48002 and 48010 UDP. Its administrative port 47990 is deliberately closed (`nix/devices/rg353m/sunshine-host.nix:83-127`). Odin composes the generic `sunshine-korri` package with `auto` capture and encoder (`nix/devices/odin2portal/web-session.nix:52-53`) and leaves `openFirewall` at its default `true` (`korri-linux-host.nix:620-623`). R36TMAX, RGDS and RP Mini V2 set `openFirewall = false` and assert it. R36TMAX has no H.264 encoder: "Mainline exposes no H.264 encoder for this variant" (`nix/devices/r36tmax/CODEC-PATH.md:102,109`). RP Mini V2 also force-disables both the Sunshine service and the certificate socket (`nix/devices/rpminiv2/portal.nix:111-133`).

RG353M is the only device with recorded encoder evidence. Its comment records software x264 at 170 to 180 percent CPU and 72 C at 640x480@30, which is why it uses RKMPP.

### Plugin host coverage is partial

Only RG353M, RGDS and RP Mini V2 import the plugin host module (`nix/devices/rg353m/plugin-host.nix`, `nix/devices/rgds/default.nix:8`, `nix/devices/rpminiv2/game-plugins.nix:39`). Odin, R36TMAX and RG35XXSP do not. Ticket 07 puts the plugin host in the product module, so that gap closes there, not here.
