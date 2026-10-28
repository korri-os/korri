# Odin 2 Portal system update — 2026-10-28

## Result: accepted and left running on the new system

| Item | Before | After |
|---|---|---|
| Main source | — | `d57a471450e4d04c770374a968ea2fc68fe355f0` (`fix(federation): preserve native peer readiness during iroh recovery`) |
| Booted/current/profile system | 11: `0ad6q5iysgrn8r7l2m87ldq5asqi96m3-nixos-system-odin2portal-sd-card-26.11.20260923.4975466` | 12: `10pbiykrs9yh49r5z1pf9yzjnxs5p4fd-nixos-system-odin2portal-sd-card-26.11.20260923.4975466` |
| `system-odin2portal-previous` | `vfz44w28…` | `0ad6q5iysgrn8r7l2m87ldq5asqi96m3…` |
| Active service bundle | `wynnbbwm1pcqq0raf4hi1rwxd1mzcyyi-korri-bundle-0.0.0` | `h5g0n8gcgpn90mmnvbmd9idyl99jakq1-korri-bundle-0.0.0` (Odin `initialPackage`) |
| Previous service bundle (rollback) | `c1z3nvnp…` | `wynnbbwm1pcqq0raf4hi1rwxd1mzcyyi-korri-bundle-0.0.0` |
| Running korrid | `36p1r33p82n58ksw2x1qa7i5si0hafmc-korrid-0.0.0` | `60xj4kiplin1mwssyl3yvjswgx8h3p35-korrid-0.0.0` (equals the bundle's `bin/korrid`) |
| Retained ESP generations | 9, 10, 11 | 10, 11, 12 (9 entries including development and Linux 7.0.2 rescue) |
| Failed units | None | None |
| loader_a SHA-256 | `dd26994c…d72389a` | identical |
| Plugin trust / device identity | `da43cc5f…` / `6f26058f…` | unchanged |
| Purpose | — | #313 `KORRID_STUN`, #315 korrid reach fix (no kernel change) |

## Scope

The owner approved one internal-storage system update for this device for this
run (2026-10-28), for this commit only. The writes were ordinary files on
`/dev/sda18` (vfat, `ODIN2P_ESP`) and `/dev/sda19` (ext4, `ODIN2P_ROOT`) through
the README's native operations. No GPT, partition, raw-block, firmware,
EFI-variable or bootloader-lock operation. No build on the device. The gate file
`/tmp/reach-315/tests-ok` existed before the first device write.

## Steps and checks

| Step | Result |
|---|---|
| Detached worktree; `git rev-parse HEAD` = `d57a471450e4…` | Verified before the build |
| Toplevel build on Zao/fuji | `10pbiykr…`; `KORRID_STUN` in its `korrid.service` |
| Bundle | `/tmp/reach-315/bundle-odin2portal` → `h5g0n8gc…`, equal to `nix eval …odin2portal.config.services.korriBundle.initialPackage.outPath` at `d57a471`; in the system closure |
| Read-only preflight | Board `AYN Odin 2 Portal`; `/boot` `/dev/sda18` vfat `ODIN2P_ESP`; `/` `/dev/sda19` ext4 `ODIN2P_ROOT`; UFS `1d84000.ufshc`; systemd-boot 261.2 from `BOOTAA64.EFI` on PARTUUID `16dd55a8…`; no running `korri-game-*` unit |
| Boot builder | `CAN_TOUCH_EFI_VARIABLES = "" == "1"` (false) |
| Sign | `nix store sign --recursive` with the Zao key: 52 signatures added |
| Closure | 1888 paths, 70 missing, 184,709,816 NAR bytes; root had 47,854,731,264 B free |
| `nix copy` | Copied; `nix-store --verify-path` OK |
| `nix-env -p …/system-odin2portal-previous --set OLD`, `nix-env -p …/system --set NEW`, `env -u NIXOS_INSTALL_BOOTLOADER -u NIXOS_INSTALL_GRUB NEW/bin/switch-to-configuration boot` | Exit 0. Both systemd-boot EFI binaries skipped as already current |
| EFI variable inventory (names + content sha256) | `f5f3031b…829d8` before and after staging. After the reboot `046eaa16…be3b2` (34 files both times): firmware/loader runtime updates at boot, as on 2026-10-22 |
| `NEW/sw/bin/korri-bundle-select switch h5g0n8gc…` | Exit 0 in 5.0 s; running korrid = bundle `bin/korrid` |
| `systemctl reboot` | SSH returned after 85 s with a new boot ID; current entry `nixos-95c6afaa…` (generation 12) |
| Services after boot | korrid, korri-inputd, korri-plugin-host, korri-chromium-kiosk, korri-compositor, inputplumber, nginx active; `systemctl --failed` empty. (Before the update, korri-inputd and korri-chromium-kiosk were inactive.) |
| Space | ESP 964,616,192 B free (unchanged; same kernel/initrd payloads); root 47,624,876,032 B free |

ESP changes: generation 9's three BLS entries were pruned by
`configurationLimit = 3`; generation 12's three entries were added;
`loader.conf` default moved to `nixos-95c6afaa…`. Payload files are unchanged.
Generation 11 entries and its profile root remain.

## korrid environment

`systemctl show korrid -p Environment` contains:

```text
"KORRID_STUN=[\"stun.cloudflare.com:3478\",\"stun.l.google.com:19302\"]"
```

## STUN evidence

korrid does not log STUN results (see `stun.rs::discover` and
`Controller::ensure_endpoint`); `korri --json status` has no reach-candidate
field, and `journalctl -b -u korrid` has no line that matches `stun`,
`external` or the public address.

Packet evidence instead: a prebuilt `tcpdump-4.99.6` (cache.nixos.org-signed,
copied, not built) captured `wlp1s0` UDP 3478/19302 into `/run` during one
`systemctl restart korrid` (04:49:08 UTC). Responses decoded with the same rules
as `stun.rs::decode`:

```text
04:49:13.161 192.168.1.103:49021 -> 162.159.207.0:3478   Binding request
04:49:13.167 162.159.207.0:3478  -> 192.168.1.103:49021  Binding success, XOR-MAPPED-ADDRESS=<public IPv4>:49021
04:49:13.174 192.168.1.103:49021 -> 74.125.250.129:19302 Binding request
04:49:13.205 74.125.250.129:19302 -> 192.168.1.103:49021 Binding success, XOR-MAPPED-ADDRESS=<public IPv4>:49021
```

Both servers returned the same public IPv4 (redacted here; it equals Zao's own
egress address and the Mini's mapping). Afterwards `ss -uanp` showed the new
korrid PID bound on UDP `49021` on every non-VPN address. Verified: korrid sent
STUN to both configured servers and got a public IPv4 mapping. Inferred from
code: korrid added that address to the iroh endpoint.

## Federation

`korri --json status` at 04:57 UTC: `rpminiv2` `ready` (and haku, rg35xxsp,
zao, rg35xxpro ready; `a808f8ac…` `failed`, as it was before the update). The
Mini reports `odin2portal` `ready`.

## Not checked

No real game launch, portal byte comparison, uclamp/limits or controller test
in this run. These were outside the requested acceptance. Physical SD boot
remains untested.

## Rollback

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.4.1 \
  '/run/current-system/sw/bin/korri-bundle-select switch /nix/store/wynnbbwm1pcqq0raf4hi1rwxd1mzcyyi-korri-bundle-0.0.0 /run/current-system/sw/bin/systemctl'
ssh -o BatchMode=yes -p 2222 root@10.42.4.1 \
  'nix-env -p /nix/var/nix/profiles/system --set /nix/store/0ad6q5iysgrn8r7l2m87ldq5asqi96m3-nixos-system-odin2portal-sd-card-26.11.20260923.4975466 &&
   env -u NIXOS_INSTALL_BOOTLOADER -u NIXOS_INSTALL_GRUB /nix/store/0ad6q5iysgrn8r7l2m87ldq5asqi96m3-nixos-system-odin2portal-sd-card-26.11.20260923.4975466/bin/switch-to-configuration boot'
ssh -o BatchMode=yes -p 2222 root@10.42.4.1 reboot
```
