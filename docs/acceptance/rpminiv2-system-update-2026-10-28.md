# Mini V2 internal-UFS system update — 2026-10-28

## Result: accepted and left running on the new system

| Item | Before | After |
|---|---|---|
| Main source | — | `d57a471450e4d04c770374a968ea2fc68fe355f0` (`fix(federation): preserve native peer readiness during iroh recovery`) |
| Booted/current/profile system | `l09c9irhqvvxnac5kksq561xh0915265-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466` (generation 34) | `5wnbwqd7jnhn573ir8499d2fxddkhq97-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466` (generation 35) |
| Fallback system (`system-rpminiv2-previous`) | `ddif9mkl…` | `l09c9irh…` |
| Active service bundle | `pqffy3kwr7m2sc7yhhgl304wjhb08cr3-korri-bundle-0.0.0` | `1b85fj7krc6hlli0j4kyhgz3pmx1bkwq-korri-bundle-0.0.0` (Mini V2 `initialPackage`) |
| Previous service bundle (rollback) | `ahbdrz63…` | `pqffy3kwr7m2sc7yhhgl304wjhb08cr3-korri-bundle-0.0.0` |
| Running korrid | `36p1r33p82n58ksw2x1qa7i5si0hafmc-korrid-0.0.0` | `60xj4kiplin1mwssyl3yvjswgx8h3p35-korrid-0.0.0` (equals the bundle's `bin/korrid`) |
| Kernel Image sha256 (running, from `BOOT_IMAGE`) | `3692c7ea…20aa3e` | `cc237f3606e17a66a60259aaeedf9b09a934e39642c490e8e10c2b17145c22b6` (#314 pin) |
| `/proc/sys/kernel/hung_task_timeout_secs` | absent | `120` |
| Failed units | None | None |
| Purpose | — | #314 kernel (hung-task/soft-lockup detectors, NETCONSOLE), #313 `KORRID_STUN`, #315 korrid reach fix |

## Scope

The owner approved one internal-storage system update for this device for this
run (2026-10-28), for this commit only. Only `nix run .#rpminiv2-update` wrote
boot files: ordinary files on the ESP (`/dev/sda21`, vfat, `RPMINIV2`) and root
(`/dev/sda22`, ext4, `NIXOS_RPMINIV2`). No GPT, raw-block, firmware-partition,
EFI-variable or GRUB-install write. No build ran on the device (`max-jobs = 0`,
`builders =`, `fallback = false`, `require-sigs = true` before and after).

The gate file `/tmp/reach-315/tests-ok` (korrid federation 130, netplay 22,
saves::peer 33, network::nearby 12 passed on `d57a471`) existed before the first
device write.

## Steps and checks

| Step | Result |
|---|---|
| Detached worktree; `git rev-parse HEAD` = `d57a471450e4…` | Verified before every build |
| `nix run .#rpminiv2-check` | Exit 0 |
| `nix run .#rpminiv2-update-check` | 8 tests OK |
| Toplevel build on Zao/fuji | `5wnbwqd7…`; `kernel` → `ijnv1g3z…-linux-…-7.2/Image`, sha256 `cc237f36…c22b6` |
| Bundle | `/tmp/reach-315/bundle-rpminiv2` → `1b85fj7k…`, equal to `nix eval …rpminiv2.config.services.korriBundle.initialPackage.outPath` at `d57a471`; in the system closure |
| `nix run .#rpminiv2-update -- --expected-system …l09c9irh…` | Signed, copied, staged. `grub.cfg`, Image, initrd, DTB, `BOOTAA64.EFI`, font: `OK`. `New system: …5wnbwqd7…` |
| `systemctl reboot` | SSH returned after 67 s, new boot ID |
| `--status` after boot | `current=` and `profile=` `5wnbwqd7…`; `previous_profile=` `l09c9irh…` |
| `korri-bundle-select switch 1b85fj7k…` | Exit 0 in 5.9 s; `active=1b85fj7k…`, `previous=pqffy3kw…`; running korrid exe = bundle `bin/korrid` |
| Services | korrid, korri-inputd, korri-plugin-host, korri-chromium-kiosk, korri-compositor, inputplumber, nginx active; `systemctl --failed` empty |
| Kernel config (`/proc/config.gz`) | `CONFIG_DETECT_HUNG_TASK=y`, `CONFIG_SOFTLOCKUP_DETECTOR=y`, `CONFIG_NETCONSOLE=y` |
| Plugin trust / device identity | `publishers.json` `e85accc5…` and `device.key` `127e6c9f…` unchanged |
| #314 hang watcher | Restarted from `/tmp/mini-314` (`watch-start.remote.sh`); `korri-hang-watch-314` active, host receiver logging heartbeats |
| Space after update | ESP 253,288,448 B free (53% used); root 13,097,906,176 B free (89% used) |

ESP changes (sha256): `grub.cfg` `01689a26…` → `d418138c…`; `grub.cfg.previous`
now holds the exact previous config `01689a26…`. Added payloads:

```text
cc237f3606e17a66a60259aaeedf9b09a934e39642c490e8e10c2b17145c22b6  /boot/EFI/nixos/ijnv1g3zh64mbxvfs13jisbmm0p5wrir-linux-aarch64-unknown-linux-gnu-7.2-Image
bcb28ccdb1691fad60dcc69e7901d991352203794e3f704c33e4a3d3a710383f  /boot/EFI/nixos/aqznd658c900riv8slhdppwjgdfjapk7-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  /boot/EFI/nixos/ijnv1g3zh64mbxvfs13jisbmm0p5wrir-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
```

The old payloads (`r0v0g05w…` Image/DTB, `rjxv501c…` initrd) remain for the
`(previous)` GRUB entry.

## korrid environment

`systemctl show korrid -p Environment` contains:

```text
"KORRID_STUN=[\"stun.cloudflare.com:3478\",\"stun.l.google.com:19302\"]"
```

## STUN evidence

korrid does not log STUN results. `stun.rs::discover` and its caller
`Controller::ensure_endpoint` (`federation/reach/controller.rs`) pass the
mapped addresses to `endpoint.add_external_addr` without a log line, and
`korri --json status` has no reach-candidate field. `journalctl -b -u korrid`
has no line that matches `stun`, `external` or the public address.

Packet evidence instead: a prebuilt `tcpdump-4.99.6` (cache.nixos.org-signed,
copied, not built) captured `wlp1s0` UDP 3478/19302 into `/run` across the
bundle switch's korrid restart. The responses were decoded with the same
rules as `stun.rs::decode`:

```text
04:53:56.107 192.168.1.143:52004 -> 162.159.207.0:3478   Binding request
04:53:56.113 162.159.207.0:3478  -> 192.168.1.143:52004  Binding success, XOR-MAPPED-ADDRESS=<public IPv4>:52004
04:53:56.123 192.168.1.143:52004 -> 74.125.250.129:19302 Binding request
04:53:56.153 74.125.250.129:19302 -> 192.168.1.143:52004 Binding success, XOR-MAPPED-ADDRESS=<public IPv4>:52004
```

Both servers returned the same public IPv4 (redacted here; it equals Zao's own
egress address). Afterwards `ss -uanp` showed korrid (new PID) bound on UDP
`52004` on every non-VPN address, which is the same-port iroh bind that follows
`discover`. Verified: korrid sent STUN to both configured servers and got a
public IPv4 mapping. Inferred from code: korrid added that address to the iroh
endpoint.

## Federation

`korri --json status` at 04:57 UTC: `odin2portal` `ready` (and haku, rg35xxsp,
zao, rg35xxpro, `a808f8ac…` ready). The Odin reports `rpminiv2` `ready`.

## Not checked

No real game launch, portal byte comparison, uclamp/limits or controller test
in this run. These were outside the requested acceptance.

## Rollback

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 \
  '/run/current-system/sw/bin/korri-bundle-select switch /nix/store/pqffy3kwr7m2sc7yhhgl304wjhb08cr3-korri-bundle-0.0.0 /run/current-system/sw/bin/systemctl'
nix run .#rpminiv2-update -- --rollback --expected-system /nix/store/5wnbwqd7jnhn573ir8499d2fxddkhq97-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 reboot
```
