# Mini V2 internal-UFS system update — 2026-10-27

## Result: accepted and left running on the new system

| Item | Final result |
|---|---|
| Main source | `ea4868c8b` (`media: iris: gen1: separate visible encoder size from storage`) |
| Booted/profile system | `ddif9mkl60ks731xi5yqyfy1s421vw0g-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466` |
| Fallback system (`previous_profile`) | `2rkkkbylfgq7zbrg2sxq7fakqwgbhjrc-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466` |
| Active service bundle | `sksv60b5p9qzingfnqzbh5wqv56dmhf0-korri-bundle-0.0.0` (Mini V2 `initialPackage`) |
| Previous service bundle (rollback) | `m1akz83jr07vpv2i2qziyq2055lbr8mv-korri-bundle-0.0.0` |
| Failed units after boot | None |
| Purpose | korri-os/korri#53: installed Iris gen1 encoder module carries the visible-size fix |

## Scope

The owner approved one Mini V2 update on 2026-10-27. Only
`nix run .#rpminiv2-update` wrote to the device: ordinary files on the ESP
(`/dev/sda21`, `RPMINIV2`) and root (`/dev/sda22`, `NIXOS_RPMINIV2`). There
were no GPT, raw-block, firmware-partition or GRUB-install writes.

## Steps and checks

| Step | Result |
|---|---|
| `nix run .#rpminiv2-check` on `ea4868c8b` | Exit 0 |
| `nix run .#rpminiv2-update-check` | 8 tests OK |
| `nix run .#rpminiv2-update -- --expected-system …2rkkkbyl…` | Staged; `grub.cfg`, Image, initrd, DTB, `BOOTAA64.EFI`, font hashes OK |
| `ssh … reboot`, then `--status` | `current=` and `profile=` equal `ddif9mkl…`; `previous_profile=` is `2rkkkbyl…` |
| `korri-bundle-select switch sksv60b5…` | `active=sksv60b5…`; running korrid exe equals the bundle's `bin/korrid` (`6qbbs63v…-korrid-0.0.0`) |
| Services | korrid, korri-plugin-host, korri-display-idle active; Sway running; portal window present |
| Iris module | Loaded from the booted system; sha256 `36e04d76…8278b`, identical to the module accepted at runtime for #53 |
| Space after update | ESP 60 MiB free (89%); root 3.9 GiB free (97%) |

## #53 check on the installed system

Sunshine plugin build with the #272 fix (`xn7czk12…-korri-korri-sunshine.service`),
loaded through the A/B drop-in, 15 s Moonlight streams, then restored:

| Stream | Content frames | Green edge frames |
|---|---|---|
| 1280x720 | 769 | 0 |
| 1240x1080 | 772 | 0 |

Afterwards: `@korri:sunshine` Disabled, Sunshine inactive, no test drop-in,
no `sunshine_state.json`.

## Open

- #284: display commits stall during every Sunshine KMS stream
  (`dpu_encoder_wait_for_commit_done`, 7 blocked-task reports in these two
  streams, none in Iris). The device hung once at Sunshine stop on 2026-10-27.
- Root is 97% full; the ESP has 60 MiB left. The next update may refuse on
  space. Clean up before it.

## Rollback

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 \
  '/run/current-system/sw/bin/korri-bundle-select switch /nix/store/m1akz83jr07vpv2i2qziyq2055lbr8mv-korri-bundle-0.0.0 /run/current-system/sw/bin/systemctl'
nix run .#rpminiv2-update -- --rollback --expected-system /nix/store/ddif9mkl60ks731xi5yqyfy1s421vw0g-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 reboot
```
