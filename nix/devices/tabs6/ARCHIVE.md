# tabs6 repository retirement, 2026-11-24

The local repository `/home/simonwjackson/code/sandbox/tabs6` is archived at
`archived-2026-11-24`. Nothing in it was deleted. Korri's
`nix/devices/tabs6` is the maintained device tree. Inventory checked against
the old tracked tree at `97c2734` and its previously untracked diagnostics.

| Old material | Disposition and reason |
| --- | --- |
| `nix/kernel/` | Already moved into `kernel/` on 2026-11-17. Korri adds the product config and subsequent driver fixes; do not restore the old recipe over them. |
| `nix/system/firmware/` | Already moved into `firmware/`. Staging now also roots all three stock inputs on zao. |
| `nix/system/{device,esp,images,modules}` | Already moved into the local `system/`, `esp/`, `images/`. Korri's deploy and boot-good paths supersede the old updater and ad hoc fsck scripts. |
| `nix/bootimg/` | Retained as archived reference. The boot container patcher/checker sources remain useful for a future rebuild, but `build-boot` packages the signed-stock-vbmeta variant that ABL rejected. The accepted v2 archive and the actual offline gate are documented in `RECOVERY.md`; no unsafe build app was imported. |
| `nix/installer/`, installer ESP variant, `install-userdata.sh`, `simg-stream.py` | Archived, superseded by the proven Mu fastboot route. No installer TCP writer is added to Korri. |
| `scripts/flash-*.sh`, `probe-fastboot.py`, `sparse-header.py` | Procedures and safety lessons moved to `RECOVERY.md`. Scripts stay archived: they contain obsolete output paths, fixed historical hashes, or lack read-back verification. Do not execute them as current recovery automation. |
| `AGENTS.md` | Partition rules, parent-only flashing, boot facts, Mu/Odin lessons and recovery hazards moved to `RECOVERY.md`. No device-local AGENTS file pattern exists in Korri; the README links the procedure. |
| `scripts/typec-transitions.sh` | Moved to `scripts/`, with a Nix shebang. Read-only journal transition summary. |
| `scripts/smw-audio-cycle.sh` | Moved to `scripts/`, with a Nix shebang. Active launch/end experiment, not read-only. It now rejects invalid counts and stops on CLI errors. |
| `scripts/input-chain.sh`, `gamesir-status.sh` | Moved to `scripts/`, with Nix shebangs. Read-only controller diagnostics; GameSir address can be passed as an argument. |
| `scripts/korri-join.exp` | Archived only. Hard-coded two-device addresses, automatic acceptance and a background join process are not a safe general procedure. Use the current `korri join` CLI interactively. |
| `scripts/korri-reset-identity.sh` | Archived only. Deletes old `/var/lib/korrid` and person state with a partial backup and without exclusive identity authority. Current identity storage and offline commands live in `services/korrid/src/identity_cli.rs`; this script must not be reused. |
| Other console/display/GPU/UFS/input/polkit/capture scripts | Archived as one-off experiments. Current product modules, acceptance records, Bluetooth pairing helper, boot-good and offline fsck replace their setup work. Timed captures, signal experiments and console test patterns are not product tooling. |
| `firmware/`, `uefi/`, `tools/`, `upstream/`, `out/` | Kept in place, ignored and not committed. Firmware and patched boot outputs are proprietary. `out/boot-uefi-v2.tar.md5`, the UEFI zip, reference analyzer and samloader remain available for parent-led recovery; do not delete the archive directory. |

## Diagnostics

Run the moved scripts **on the tablet**, only when your session owns it.
They are not host probes. Direct execution uses a Nix shebang; `bash -s` over
SSH uses the tablet's installed tools and does not invoke Nix there. Do not
build or evaluate a Nix shell on the tablet.

```sh
ssh -p 2222 root@tabs6.local bash -s < nix/devices/tabs6/scripts/typec-transitions.sh
ssh -p 2222 root@tabs6.local bash -s < nix/devices/tabs6/scripts/input-chain.sh
ssh -p 2222 root@tabs6.local bash -s < nix/devices/tabs6/scripts/gamesir-status.sh
# Active experiment: launches and ends games; never run during another session.
ssh -p 2222 root@tabs6.local bash -s 3 < nix/devices/tabs6/scripts/smw-audio-cycle.sh
```

No tablet access, flash, deployment or real garbage collection was used for
this retirement.

## Korri worktree cleanup

Only the nine named tabs6 leftovers were removed, after checking their clean
working trees and `git cherry -v main <branch>`. Other sessions' worktrees
were not changed.

| Branch / worktree | Evidence |
| --- | --- |
| `tabs6-typec`, `tabs6-typec-phase-b`, `tabs6-gpu-fix`, `tabs6-typec-safe-observe`, `tabs6-gamesir-reconnect` | Every commit was patch-equivalent to main (`git cherry` reported only `-`). |
| `tabs6-typec-d` | Same, except deliberately unlanded `9241eed5fe2072dc3eddb28d9226e7f892979aa3`. Preserved first with annotated tag `tabs6-typec-capture-diagnostic`; not merged. |
| `tabs6-audio-stall` | `1488e4faf` reported `+`, but its acceptance record, patch, test and kernel recipe match landed `68291af0d` byte-for-byte. The only tree difference in the touched paths is the unrelated `tabs6-deploy` check present on the latter's base. Superseded, not unique work. |
| `c-70ccc5ef…` (sink-host), `c-ae7ae6d2…` (Kishi latency) | All commits patch-equivalent to main. No branches literally named `tabs6-sink-host` or `tabs6-kishi-latency` remained. |

The GameSir worktree was at
`/tmp/forgerie-scratch/f-6594ef25/tabs6-gamesir-reconnect`; the other eight
were the specified child worktrees under Korri's `.worktrees/c-…`. There
were no additional unique unlanded commits to keep. The diagnostic tag keeps
the intentionally unlanded capture service reachable without enabling it.
