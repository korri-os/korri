# Odin 2 Portal system update — 2026-10-28 (second run)

## Result: accepted and left running on the new system

| Item | Before | After |
|---|---|---|
| Main source | — | `6bf379ccdbda258bc9694ea0fefab4c76846f3bd` (`fix(product): ship five working Nostr relays by default (#315)`) |
| Booted/current/profile system | 12: `10pbiykrs9yh49r5z1pf9yzjnxs5p4fd-nixos-system-odin2portal-sd-card-26.11.20260923.4975466` | 13: `33d6bak5sflwdr8qxk4fi3n3nxmjbbsk-nixos-system-odin2portal-sd-card-26.11.20260923.4975466` |
| `system-odin2portal-previous` | `0ad6q5iy…` | `10pbiykrs9yh49r5z1pf9yzjnxs5p4fd…` |
| Active service bundle | `gs0iba2875rg13p2h0f6w7hq9m9dvg2g-korri-bundle-0.0.0` | unchanged: it is already the Odin `initialPackage` at `6bf379c` |
| Previous service bundle | `37b5qc0a…` | unchanged |
| Running korrid | `ygr5w8lbx5qkk8p6d7afmsq3473bwsp1-korrid-0.0.0` | same path, new PID after boot (equals the bundle's `bin/korrid`) |
| `KORRID_RELAYS` (korrid `/proc/<pid>/environ`) | `["wss://relay.nostr.band","wss://relay.primal.net"]` | `["wss://offchain.pub","wss://nostr-pub.wellorder.net","wss://relay.primal.net","wss://nos.lol","wss://nostr.oxtr.dev"]` |
| `KORRID_STUN` | both servers | unchanged: `["stun.cloudflare.com:3478","stun.l.google.com:19302"]` |
| Retained ESP generations | 10, 11, 12 | 11, 12, 13 (9 entries including development and Linux 7.0.2 rescue) |
| Identity (`korri --json status`) | `Owned`, device `6544cbd4…` (full key below), owner `a80db13fd36d6e15cb45df12db658da04dd6dc7c6d9200dcebabdcb207ed3dca` | identical |
| Failed units | None | None |
| loader_a SHA-256 | `dd26994c…d72389a` | identical |
| Plugin trust (`/etc/korri-plugin-host/publishers.json`) | `da43cc5f…` | unchanged |
| Purpose | — | #315 five-relay default (no kernel change) |

Device public key before and after: `6544cbd41e84cb4c2c1bf4391643551e5b9f0c4d27c140046a85369bfa0475ab`.

## Scope

The owner approved one internal-storage system update for this device for this
run (2026-10-28, "install based on the latest from main"), for this commit only.
The writes were ordinary files on `/dev/sda18` (vfat, `ODIN2P_ESP`) and
`/dev/sda19` (ext4, `ODIN2P_ROOT`) through the README's native operations. No
GPT, partition, raw-block, firmware, EFI-variable or bootloader-lock operation.
No build on the device. No identity, key or `/var/lib/korri*` file was touched;
no identity command ran.

## Steps and checks

| Step | Result |
|---|---|
| Detached worktree; `git rev-parse HEAD` = `6bf379ccdbda…` | Verified before the build |
| Toplevel build on Zao/fuji/yari | `33d6bak5…`; its `korrid.service` carries the five relays and both STUN servers |
| Bundle | `nix build …odin2portal.config.services.korriBundle.initialPackage` → `gs0iba28…`, equal to `nix eval …initialPackage.outPath`; in the system closure; equal to the active bundle |
| Read-only preflight | Board `AYN Odin 2 Portal`; `/boot` `/dev/sda18` vfat `ODIN2P_ESP`; `/` `/dev/sda19` ext4 `ODIN2P_ROOT`; UFS `1d84000.ufshc`; systemd-boot 261.2 from `BOOTAA64.EFI` on PARTUUID `16dd55a8…`; no running `korri-game-*` unit |
| Boot builder | `CAN_TOUCH_EFI_VARIABLES = "" == "1"` (false) |
| Sign | `nix store sign --recursive` with the Zao key: 26 signatures added |
| Closure | 1888 paths, 27 missing, 27,874,552 NAR bytes; root had 47,811,059,712 B free |
| `nix copy` | Copied; `nix-store --verify-path` OK |
| `nix-env -p …/system-odin2portal-previous --set OLD`, `nix-env -p …/system --set NEW`, `env -u NIXOS_INSTALL_BOOTLOADER -u NIXOS_INSTALL_GRUB NEW/bin/switch-to-configuration boot` | Exit 0. Both systemd-boot EFI binaries skipped as already current |
| EFI variable inventory (names + content sha256, 34 files) | `3c8e7bab…d948` before and after staging. After the reboot `63fe3a89…467f` (34 files): firmware/loader runtime updates at boot, as on 2026-10-22 and this morning |
| `systemctl reboot` (19:39:18 UTC) | SSH returned at 19:40:30 with new boot ID `3301c5fa…`; current entry `nixos-714fee5e…` (generation 13) |
| Bundle switch | Not run: the built bundle equals the active bundle |
| Services after boot | korrid, korri-inputd, korri-plugin-host, korri-chromium-kiosk, korri-compositor, inputplumber, nginx active; `systemctl --failed` empty |
| Plugins | 33 receipts (32 Enabled, 1 Disabled). Per receipt: id, desired state, package, provenance and `selection.json` sha256; `korri-plugin status ID` for each; `korri-plugin enabled-packages`; plugin units. Before and after outputs are byte-identical |
| Space | ESP 964,616,192 B free (unchanged; same kernel/initrd payloads); root 47,749,763,072 B free |

ESP changes: generation 10's three BLS entries were pruned by
`configurationLimit = 3`; generation 13's three entries were added;
`loader.conf` default moved from `nixos-95c6afaa…` to `nixos-714fee5e…`.
Payload files are unchanged. Generation 12 entries and its profile root remain.

## korrid environment

From `/proc/$(systemctl show korrid -p MainPID --value)/environ`:

```text
KORRID_RELAYS=["wss://offchain.pub","wss://nostr-pub.wellorder.net","wss://relay.primal.net","wss://nos.lol","wss://nostr.oxtr.dev"]
KORRID_STUN=["stun.cloudflare.com:3478","stun.l.google.com:19302"]
```

## Reach and relays

korrid started before Wi-Fi was up. Its first STUN attempts logged
`error=dns_failed`, and its first 12 publishes failed on all five relays. Two
`network_reset` events followed; after them, STUN succeeded on both servers
(public IPv4 redacted):

```text
reset=2 stage=stun_result server=stun.cloudflare.com:3478 mapped=38.x.x.x:59014 server_ms=10
reset=2 stage=stun_result server=stun.l.google.com:19302 mapped=38.x.x.x:59014 server_ms=34
reset=2 stage=endpoint_bound port=59014
```

`journalctl -b -u korrid | grep 'korrid: reach:'` at 19:42 UTC: every publish
line has `relay_count=5`. Per-relay outcomes of the 28 final (`final=true`)
offer/answer publishes after the network came up:

| Relay | ok | failed |
|---|---:|---:|
| `wss://relay.primal.net` | 28 | 0 |
| `wss://nostr.oxtr.dev` | 28 | 0 |
| `wss://nos.lol` | 7 | 21 |
| `wss://offchain.pub` | 0 | 28 |
| `wss://nostr-pub.wellorder.net` | 0 | 28 |

The Mini acceptance file records the same pattern and the HTTPS reachability
check. After the reset, the Odin made direct connections to the Mini
(`connection_established … peer=8cd28f8a path=direct_v4` and `direct_v6`).

## Federation

`korri --json status` at 19:42 UTC: `rpminiv2` `ready` (also haku, rg35xxsp,
zao, rg35xxpro `ready`; `a808f8ac…` `failed`). The Mini reports `odin2portal`
`ready`.

## Not checked

No real game launch, portal byte comparison, uclamp/limits or controller test
in this run. These were outside the requested acceptance. Physical SD boot
remains untested.

## Rollback

The bundle did not change, so only the system needs to roll back:

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.4.1 \
  'nix-env -p /nix/var/nix/profiles/system --set /nix/store/10pbiykrs9yh49r5z1pf9yzjnxs5p4fd-nixos-system-odin2portal-sd-card-26.11.20260923.4975466 &&
   env -u NIXOS_INSTALL_BOOTLOADER -u NIXOS_INSTALL_GRUB /nix/store/10pbiykrs9yh49r5z1pf9yzjnxs5p4fd-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/bin/switch-to-configuration boot'
ssh -o BatchMode=yes -p 2222 root@10.42.4.1 reboot
```
