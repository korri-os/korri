# RG35XX SP, RG35XX Pro and RG353M system update to `6bf379c` — 2026-10-28

## Result

| Device | Result |
|---|---|
| RG35XX SP (`rg35xxsp`, 10.42.5.1) | Accepted. Runs the `6bf379c` system and bundle. |
| RG35XX Pro (`rg35xxpro`, 10.42.6.1) | Acceptance failed. Rolled back to generation 49 and its bundle. |
| RG353M (`haku`, 10.42.0.1) | Accepted. Runs the `6bf379c` system and bundle. |

Source: `6bf379ccdbda258bc9694ea0fefab4c76846f3bd` (`fix(product): ship five
working Nostr relays by default (#315)`), `origin/main` on 2026-10-28. The owner
approved one system update per device at this commit on 2026-10-28.

## Before and after

| Item | SP before | SP after | Pro before = after (rolled back) | haku before | haku after |
|---|---|---|---|---|---|
| Current = profile | gen 13 `jxcgxaw5…` | gen 14 `r2jgp9i3…` | gen 49 `vaniwvjb…` | gen 37 `c9pj6jx9…` | gen 38 `qk4fw14n…` |
| Booted | `jxcgxaw5…` | `jxcgxaw5…` (no reboot) | `vaniwvjb…` | `z92z5l69…` | `c9pj6jx9…` (thermal reboot, see below) |
| Active bundle | `19cgkv6d…` | `ai5kcw2i…` | `19cgkv6d…` | `6klw1psk…` | `9g3l8gga…` |
| Previous bundle | `ajlbjlpr…` | `19cgkv6d…` | `ajlbjlpr…` | `xzajzv99…` | `6klw1psk…` |
| korrid executable | `fbvsb7l5…` | `ygr5w8lb…` (= bundle `bin/korrid`) | `fbvsb7l5…` | `zrpzi8rs…` | `ygr5w8lb…` (= bundle `bin/korrid`) |
| `KORRID_RELAYS` | 2 relays (`relay.nostr.band`, `relay.primal.net`) | the 5 relays below | 2 relays | 2 relays | the 5 relays below |
| `KORRID_STUN` | absent | set | absent | absent | set |
| Failed units | none | none | `eggshell-profile-trust.service` | none | none |
| Root free | 45.6 GB | 45.4 GB | 9.63 GB → 9.40 GB | 5.40 GB | 5.24 GB |

Full store paths:

| Output | Path |
|---|---|
| SP system | `/nix/store/r2jgp9i35k0qfg3v9kqs4fip7wf53wa6-nixos-system-rg35xxsp-sd-card-26.11.20260923.4975466` |
| Pro system (built, not kept) | `/nix/store/ski86fkg5d1ar915r7y22clr7j5kj188-nixos-system-rg35xxpro-sd-card-26.11.20260923.4975466` |
| haku system | `/nix/store/qk4fw14nacxxdjmckd21yshm2facb1fd-nixos-system-haku-sd-card-26.11.20260923.4975466` |
| SP and Pro bundle | `/nix/store/ai5kcw2iv5w9xhhwa5ddhnjx6ckgvwl8-korri-bundle-0.0.0` |
| haku bundle | `/nix/store/9g3l8gga4ba44jia4s8k0dnkdbq7isal-korri-bundle-0.0.0` |
| korrid in both bundles | `/nix/store/ygr5w8lbx5qkk8p6d7afmsq3473bwsp1-korrid-0.0.0/bin/korrid` |
| SP rollback | system `/nix/store/jxcgxaw5sa4s3rxbv28p2m8pnvk9v7r3-nixos-system-rg35xxsp-sd-card-26.11.20260923.4975466`, bundle `/nix/store/19cgkv6dwzw9s9mz7vzlcwdgknjgabvv-korri-bundle-0.0.0` |
| haku rollback | system `/nix/store/c9pj6jx9q3gx5l1mgv6yrlcg1k6a11k0-nixos-system-haku-sd-card-26.11.20260923.4975466`, bundle `/nix/store/6klw1pski4s5hdb8zq8dz1yn8b9q83v1-korri-bundle-0.0.0` |

The kernel, initrd, DTB directory and kernel parameters of each new system were
identical to the running ones, so `switch` needed no reboot.

## Procedure

The procedure is the one used for the earlier SP, Pro and haku updates:
`/tmp/sp-proof/deploy.sh` on the host and `/tmp/rg353m-hwdec/activate-haku.sh`
on the device (the 2026-10-27 haku update). `activate-haku.sh` is
`/tmp/sp-proof/activate.sh` with one change: it tolerates
`switch-to-configuration` exit 4 only if no unit failed that had not already
failed before the switch. The script was copied byte for byte (sha256
`2f9ca8d0…f12a`). The host side added one gate to `deploy.sh`: it stops if the
copy would leave less than 1 GiB free on root.

1. Detached worktree at the commit; `test "$(git rev-parse HEAD)" = 6bf379ccd…`
   passed before the build and before the output-path evaluation.
2. One `nix build` of the six outputs
   (`nixosConfigurations.{rg35xxsp,rg35xxpro,rg353m}.config.system.build.toplevel`
   and `….services.korriBundle.initialPackage`) on Zao and its builders (fuji,
   yari). Nothing was built on a device.
3. Per device, one at a time: `nix store sign --recursive` with
   `~/.config/korri/cache-key.secret`, `nix store verify --sigs-needed 1`
   against `korri-cache-zao-1`, free-space gate, `nix copy` to
   `ssh-ng://root@HOST` (`NIX_SSHOPTS='-p 2222 -o BatchMode=yes'`). Signature
   checks stayed on.
4. On the device: `nix-env --profile /nix/var/nix/profiles/system --set`,
   `switch-to-configuration switch`, `systemctl stop korrid`,
   `korri-bundle-select switch BUNDLE`, `korri-plugin restore-all`.

What the activation writes: the boot loader is
`install-extlinux-conf.sh` → `extlinux-conf-builder.sh -d /boot`. It writes only
`/boot/extlinux/extlinux.conf` and files in `/boot/nixos`. On all three devices
`/boot` is a directory on the root ext4 file system `/dev/mmcblk0p2` (on the SP
and Pro the TF1 card; on haku the internal eMMC). No activation script contains
`dd`, `sfdisk` or a `mmcblk`/`boot0` reference. No raw U-Boot area,
partition table, `mmcblk0boot0`/`boot1`, FAT `NIXOS_BOOT` partition or whole-disk
image was written. The haku `extlinux.conf` diff shows only the new
`nixos-38-*` entries and the default `init=` change; the old entries pruned by
`-g 3` were generations 35.

## Acceptance checks

| Check | SP | haku |
|---|---|---|
| current = profile = built toplevel | Verified | Verified |
| Active bundle = built bundle; korrid runs from it | Verified (`/proc/PID/exe`) | Verified |
| `KORRID_RELAYS` = `["wss://offchain.pub","wss://nostr-pub.wellorder.net","wss://relay.primal.net","wss://nos.lol","wss://nostr.oxtr.dev"]` | Verified (`systemctl show korrid`) | Verified |
| `KORRID_STUN` = `["stun.cloudflare.com:3478","stun.l.google.com:19302"]` | Verified | Verified |
| `korrid: reach:` lines with `relay_count=5` | 1,126 of 1,392 reach lines; no other `relay_count` | 297 of 404; no other `relay_count` |
| STUN | `stage=stun_result` from both servers with a mapped IPv4 | Same, after two DNS failures before Wi-Fi was up |
| Failed units | none before, none after | none before, none after |
| Identity (`korri --json status`) | `Owned`, device `306cdc65…`, owner `a80db13f…3dca`, unchanged | `Owned`, device `0aae318d…`, owner `a80db13f…3dca`, unchanged |
| Plugin set and enabled state | Same 27 IDs and flags; `enabled-packages.json` sha256 unchanged (`7f2a39f7…`) | Same 30 IDs and flags; sha256 unchanged (`5e1e8450…`) |
| Peers | haku, odin2portal, zao, rpminiv2, rg35xxpro `ready` | rg35xxsp, odin2portal, zao, rpminiv2, rg35xxpro `ready` |
| Boot from the new generation | Not tested (no reboot needed) | Not tested |

Peer `a808f8ac…` (no label) showed `failed` / `ready` alternately on all three
devices before and after; it was not part of the acceptance list.

Relay outcomes in the last ten final publishes on each device:
`relay.primal.net` and `nostr.oxtr.dev` 10/10 ok; `nos.lol` 3–5 of 10 ok;
`offchain.pub` and `nostr-pub.wellorder.net` 0/10 ok. Earlier in the SP window,
`nostr-pub.wellorder.net` was ok 87 times. korrid's reach log does not give a
reason for a failed relay.

## RG35XX Pro: acceptance failed, rolled back

`switch-to-configuration switch` exited 4. Before the switch,
`eggshell-profile-trust.service` was the only failed unit (known, since
2026-10-27). After it, `korri-plugin-host.service` also failed:

```text
korri-plugin: plugin @simonwjackson:eggshell: publisher @simonwjackson is not bound on this device; plugin @simonwjackson:pogo-pogo: publisher @simonwjackson is not bound on this device
```

Cause: generation 49 (`vaniwvjb…`) was not a `main` build. Its
`/etc/korri-plugin-host/publishers.json` binds `@simonwjackson` to
`file:///var/lib/eggshell-profile/cache` with key
`simonwjackson-plugins-1:4SchQgTM…`. On `main`, that binding exists only for
the Mini V2 and the Odin (`nix/devices/{rpminiv2,odin2portal}/owner-publisher.nix`).
The `6bf379c` Pro system binds only `@korri`. The Eggshell trust unit cannot
restore the binding either: it refuses a changed `publishers.json`, and it had
already failed before this run. Installing `main` on the Pro therefore drops
two enabled plugins.

The activation script stopped before the bundle switch, so the bundle stayed
`19cgkv6d…`. Rollback: `nix-env --profile /nix/var/nix/profiles/system --rollback`
(50 → 49), `vaniwvjb…/bin/switch-to-configuration switch` (exit 4, only
`eggshell-profile-trust.service` failed), `systemctl restart korri-plugin-host`
(active). After rollback (verified): current = booted = profile `vaniwvjb…`
(gen 49), extlinux default `init=` is `vaniwvjb…`, bundle `19cgkv6d…`, the
same 29 plugin IDs and flags, `enabled-packages.json` sha256 `ad9f728e…`
unchanged, identity `Owned`, device `92ac6eeb…`, owner `a80db13f…3dca`, all
five named peers `ready`. Generation 50 (`ski86fkg…`) stays as a non-default
extlinux menu entry and profile link.

The Pro was on the new system from 18:31:46 to about 18:37:40 UTC. korrid
restarted during the switch and the rollback (PID 37588 before, 185325 after).

## haku thermal shutdown (not caused by this update)

haku's boot ID changed between the baseline and the update. The previous boot's
journal shows a game launch (`@korri:mgba`) and a Sunshine stream session at
18:29:37 UTC, then `reboot: HARDWARE PROTECTION shutdown (Temperature too
high)` at 18:32:22 UTC. That was before the first haku write (18:40:07). The SSH
sessions just before the shutdown came from `192.168.1.243` (Zao over Wi-Fi), not
from this run (USB `10.42.0.12`). haku came back on generation 37 with
`/dev/mmcblk0p2` as root and no failed units. After the update a game unit
(`korri-game-a720dd92…`) was running; this run did not start it.

## Not checked

No reboot into the new generations, no game launch, no stream and no controller
test. Evidence is in `/tmp/sd-6bf379c/` on Zao (`before-*`, `after-*`,
`rolledback-*`, `final-*` snapshots, `deploy-*.log`, `reach-*.log`).

## Rollback commands

```sh
# SP
ssh -p 2222 root@10.42.5.1 'nix-env --profile /nix/var/nix/profiles/system --set /nix/store/jxcgxaw5sa4s3rxbv28p2m8pnvk9v7r3-nixos-system-rg35xxsp-sd-card-26.11.20260923.4975466 && /nix/store/jxcgxaw5sa4s3rxbv28p2m8pnvk9v7r3-nixos-system-rg35xxsp-sd-card-26.11.20260923.4975466/bin/switch-to-configuration switch; systemctl stop korrid; korri-bundle-select switch /nix/store/19cgkv6dwzw9s9mz7vzlcwdgknjgabvv-korri-bundle-0.0.0 /run/current-system/sw/bin/systemctl; korri-plugin restore-all'
# haku
ssh -p 2222 root@10.42.0.1 'nix-env --profile /nix/var/nix/profiles/system --set /nix/store/c9pj6jx9q3gx5l1mgv6yrlcg1k6a11k0-nixos-system-haku-sd-card-26.11.20260923.4975466 && /nix/store/c9pj6jx9q3gx5l1mgv6yrlcg1k6a11k0-nixos-system-haku-sd-card-26.11.20260923.4975466/bin/switch-to-configuration switch; systemctl stop korrid; korri-bundle-select switch /nix/store/6klw1pski4s5hdb8zq8dz1yn8b9q83v1-korri-bundle-0.0.0 /run/current-system/sw/bin/systemctl; korri-plugin restore-all'
```
