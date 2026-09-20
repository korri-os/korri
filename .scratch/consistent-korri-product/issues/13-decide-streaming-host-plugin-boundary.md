# Decide the streaming host plugin boundary

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: open
Blocked by: 07

## Question

Hosting a stream is a bundled plugin, on by default in most devices' plugin selection and removable through the plugin lifecycle, not a product part ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#streaming-host)). Today Sunshine is a systemd service composed by [korri-linux-host](../../../services/inputd/nix/korri-linux-host.nix) with a certificate control socket, seat wiring, and per-device encoder builds ([rg353m rkmpp Sunshine](../../../nix/devices/rg353m/default.nix)). [nix/device-cache/README.md](../../../nix/device-cache/README.md) states that plugin publication does not make Sunshine into a plugin; that sentence is revised by the parent decision.

What does the streaming host plugin declare, what effects must korrid own for it (certificate provisioning, seat lease, encoder selection, firewall), and which devices select it by default? Recheck the plugin boundaries rule in [AGENTS.md](../../../AGENTS.md): native programs arrive prebuilt, system configuration stays in native format, `plugin.ts` stays thin, and a plugin performs no effects. Compare with the existing Moonlight plugin direction in [Choose the opinionated plugin selection](03-choose-default-plugins.md#streaming-client) and with the RP Mini V2 case that hand-disables the units today.

Resolve with Simon the plugin's contributions, korrid's effect ownership, the per-device default selection including which devices omit it and why, and what evidence proves a device with the plugin removed still meets the product. Do not invent a plugin declaration schema; extract it from the existing plugin host receipts and the plugin briefs.
