# Mini V2 internal-UFS system update — 2026-10-28 (second run)

## Result: accepted and left running on the new system

| Item | Before | After |
|---|---|---|
| Main source | — | `6bf379ccdbda258bc9694ea0fefab4c76846f3bd` (`fix(product): ship five working Nostr relays by default (#315)`) |
| Booted/current/profile system | 35: `5wnbwqd7jnhn573ir8499d2fxddkhq97-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466` | 36: `54szpx8y9bdqd615hb2r94pjmid26z0p-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466` |
| Fallback system (`system-rpminiv2-previous`) | `l09c9irh…` | `5wnbwqd7…` |
| Active service bundle | `c4706cxp59w5ifj4gizfpmqi3bybi9l5-korri-bundle-0.0.0` | unchanged: it is already the Mini V2 `initialPackage` at `6bf379c` |
| Previous service bundle | `wnp2gvlp…` | unchanged |
| Running korrid | `ygr5w8lbx5qkk8p6d7afmsq3473bwsp1-korrid-0.0.0` | same path, new PID after boot (equals the bundle's `bin/korrid`) |
| `KORRID_RELAYS` (korrid `/proc/<pid>/environ`) | `["wss://relay.nostr.band","wss://relay.primal.net"]` | `["wss://offchain.pub","wss://nostr-pub.wellorder.net","wss://relay.primal.net","wss://nos.lol","wss://nostr.oxtr.dev"]` |
| `KORRID_STUN` | both servers | unchanged: `["stun.cloudflare.com:3478","stun.l.google.com:19302"]` |
| Kernel Image sha256 (running, from `BOOT_IMAGE`) | `cc237f36…c22b6` | `cc237f3606e17a66a60259aaeedf9b09a934e39642c490e8e10c2b17145c22b6` (unchanged, `ijnv1g3z…` Image) |
| `/proc/sys/kernel/hung_task_timeout_secs` | `120` | `120` |
| Identity (`korri --json status`) | `Owned`, device `8cd28f8a874f1eae75483ddbeedaeb4260b535598f4f4d6a852330bf7ede8e53`, owner `a80db13fd36d6e15cb45df12db658da04dd6dc7c6d9200dcebabdcb207ed3dca` | identical |
| Plugin trust (`/etc/korri-plugin-host/publishers.json`) | `e85accc5…` | `e85accc5…` |
| Failed units | None | None |
| Purpose | — | #315 five-relay default (`nix/product/requirements.nix`) |

## Scope

The owner approved one internal-storage system update for this device for this
run (2026-10-28, "install based on the latest from main"), for this commit only.
Only `rpminiv2-update` wrote boot files: ordinary files on the ESP
(`/dev/sda21`, vfat, `RPMINIV2`) and root (`/dev/sda22`, ext4,
`NIXOS_RPMINIV2`). No GPT, raw-block, firmware-partition, EFI-variable or
GRUB-install write. No build ran on the device (`max-jobs = 0`, `builders =`,
`fallback = false`, `require-sigs = true` before and after). No identity, key
or `/var/lib/korri*` file was touched; no identity command ran.

## Steps and checks

| Step | Result |
|---|---|
| Detached worktree; `git rev-parse HEAD` = `6bf379ccdbda…` | Verified before every build and before the update |
| Toplevel and bundle build on Zao/fuji/yari | `54szpx8y…`; `kernel` → `ijnv1g3z…/Image`, sha256 `cc237f36…c22b6`; `korrid.service` carries the five relays |
| Bundle | `nix build …rpminiv2.config.services.korriBundle.initialPackage` → `c4706cxp…`, equal to `nix eval …initialPackage.outPath`; in the system closure; equal to the active bundle |
| `nix run .#rpminiv2-update-check` | 8 tests OK |
| `nix run .#rpminiv2-check` | Exit 0 |
| `rpminiv2-update --status` (before) | `current=` = `profile=` = `5wnbwqd7…`; `grub_sha256=d418138c…` |
| `nix run .#rpminiv2-update -- --expected-system …5wnbwqd7…` | 24 signatures added, 25 paths copied. `grub.cfg`, Image, initrd, DTB, `BOOTAA64.EFI`, font: `OK`. `New system: …54szpx8y…` |
| `systemctl reboot` (19:34:29 UTC) | SSH returned at 19:35:13 with new boot ID `04c97566…` |
| `--status` after boot | `current=` and `profile=` `54szpx8y…`; `previous_profile=` `5wnbwqd7…` |
| Bundle switch | Not run: the built bundle equals the active bundle |
| Services | korrid, korri-inputd, korri-plugin-host, korri-chromium-kiosk, korri-compositor, inputplumber, nginx active; `systemctl --failed` empty |
| Kernel config (`/proc/config.gz`) | `CONFIG_DETECT_HUNG_TASK=y`, `CONFIG_SOFTLOCKUP_DETECTOR=y`, `CONFIG_NETCONSOLE=y` |
| Plugins | 56 receipts (51 Enabled, 5 Disabled). Per receipt: id, desired state, package, provenance and `selection.json` sha256; `korri-plugin status ID` for each; `korri-plugin enabled-packages`; plugin units. Before and after outputs are byte-identical |
| #314 hang watcher | It was not running before this update: its last heartbeat in `/tmp/mini-314/netlog/udp.log` was 10:02:29 UTC, before an earlier reboot. Restarted from `/tmp/mini-314` (`scp hang-watch.pl`, `run-remote.sh watch-start.remote.sh`) at 19:37:17; `START` and new `H` heartbeats arrive |
| Space after update | ESP 253,288,448 B free (unchanged); root 12,630,867,968 B free (90% used) |

ESP changes: `grub.cfg` `d418138c…` → `2098cfec221a0d803bd555c8050c798f7123ab679c76902226c43d7d8b5d2095`;
`grub.cfg.previous` holds the exact previous config `d418138c…`. The kernel,
initrd and DTB payloads are the same files as before (`ijnv1g3z…` Image/DTB,
`aqznd658…` initrd); no payload was added.

## korrid environment

From `/proc/$(systemctl show korrid -p MainPID --value)/environ`:

```text
KORRID_RELAYS=["wss://offchain.pub","wss://nostr-pub.wellorder.net","wss://relay.primal.net","wss://nos.lol","wss://nostr.oxtr.dev"]
KORRID_STUN=["stun.cloudflare.com:3478","stun.l.google.com:19302"]
```

## Reach and relays

korrid now logs STUN results. First boot lines (public IPv4 redacted):

```text
stage=stun_result server=stun.cloudflare.com:3478 mapped=38.x.x.x:36497 server_ms=16
stage=stun_result server=stun.l.google.com:19302 mapped=38.x.x.x:36497 server_ms=40
stage=endpoint_bound port=36497
```

`journalctl -b -u korrid | grep 'korrid: reach:'` at 19:42 UTC: every publish
line has `relay_count=5`. Per-relay outcomes of the 103 final
(`final=true`) offer/answer publishes:

| Relay | ok | failed |
|---|---:|---:|
| `wss://relay.primal.net` | 103 | 0 |
| `wss://nostr.oxtr.dev` | 103 | 0 |
| `wss://nos.lol` | 25 | 78 |
| `wss://offchain.pub` | 2 | 101 |
| `wss://nostr-pub.wellorder.net` | 0 | 103 |

Example: `stage=offer_published peer=6544cbd4 relay_count=5 completed_relays=5
outcomes=wss://relay.primal.net=ok,wss://nostr.oxtr.dev=ok,wss://offchain.pub=failed,wss://nostr-pub.wellorder.net=failed,wss://nos.lol=failed operation_ms=1882 first_ack_ms=707 final=true`.

Two of five relays answer reliably. HTTPS NIP-11 requests from the device to
`offchain.pub` and `nostr-pub.wellorder.net` return 200 and `nos.lol` returns
429, so the hosts are reachable. The korrid log does not say why a publish
failed. This contradicts the commit message's claim that all five relays
passed publish/read checks. Not investigated in this run.

## Federation

`korri --json status` at 19:42 UTC: `odin2portal` `ready` (also haku,
rg35xxsp, zao, rg35xxpro `ready`; `a808f8ac…` `failed`, "Can't reach … from
this network right now"). The Odin reports `rpminiv2` `ready`. Before the update,
rg35xxsp was `failed` on both devices.

## Not checked

No real game launch, portal byte comparison, uclamp/limits or controller test
in this run. These were outside the requested acceptance.

## Rollback

The bundle did not change, so only the system needs to roll back:

```sh
nix run .#rpminiv2-update -- --rollback --expected-system /nix/store/54szpx8y9bdqd615hb2r94pjmid26z0p-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 reboot
```
