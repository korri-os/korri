# Korri

Korri is being rebuilt from an empty mainline, one end-to-end slice at a time.

Korri targets Linux devices. Each device runs its own korrid; the local
portal mounts a surface. Devices are peers, not fixed hosts and clients.
Each slice brings in only the code it needs.

## Branches

- `main` — the new mainline. Orphan branch; history starts here.
- `legacy` — the previous product in full: NixOS kiosk, ROCKNIX/SM8550 device
  payloads, x86 live USB, web portal, plugin ecosystem. Kept as reference and
  as the source to harvest code from. Nix-on-ROCKS device support is paused,
  not deleted.
