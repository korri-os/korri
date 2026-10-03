# Controller seat normalization

## Decision

The user confirmed: "the game should just see 4 controllers connected". Ask `893ffd2a-bcd7-4551-9aff-a5be59ab7c48` selected `all-four-visible`.

Keep four persistent gamepads with the default configuration, even when no physical controller is attached. Create them before game launch. An empty seat stays connected and neutral. Neither occupied-only exposure nor demand-driven device creation is allowed.

A physical or remote controller binds to a seat. Disconnect keeps its reservation through the live session, including leave/return. Reconnect reclaims that seat. End releases disconnected reservations. Outside a live session, disconnect releases the reservation immediately. The gamepad remains present.

Explicit controller-seat changes remain a future operation. Implementing them is outside this change. Do not introduce a binding identity that makes that operation impossible.

This preserves the allocation and configuration decisions in [Unified controller input](2026-09-28-unified-controller-input.md). It does not change the existing configurable count, its device-only ownership, or idle-only resizing.

## What normalization can guarantee

Korri supplies stable controller devices, standard input state, and source-to-seat routing. The game decides which controller controls each player.

The pinned Dr. Mario 64 source collects controllers from an `unordered_map`, then fills unmatched players from that list. It does not sort the fallback by SDL player index. See [input.cpp, lines 722–791](https://github.com/theboy181/drmario64_recomp_plus/blob/af91e3bf56b1ffc329ff4327fdc2380515463de7/src/game/input.cpp#L722-L791). [main.cpp, line 704](https://github.com/theboy181/drmario64_recomp_plus/blob/af91e3bf56b1ffc329ff4327fdc2380515463de7/src/main/main.cpp#L704) disables single-controller mode.

The device capture in the source session showed physical input reaching P1. An approved account-only identity pin restored control according to the owner. The previous in-game assignment was not observed directly.

The earlier recommendation to hide unused seats is withdrawn. Four visible controllers are intentional. Changing names, IDs, or SDL mappings cannot force an application to follow their order. The account pin remains a workaround, not a shared correction. No game recipe or account configuration changes in this slice.

## Shared change

The udev seat rule previously matched only `event*`. The Linux joystick siblings, `js*`, inherited broader default permissions.

A real VM reproduced an unrelated user opening a seat's `js*` node. Its owner was root, group was input, and mode was `0664`. The gameplay user also opened it, so this was an access restriction gap, not a missing controller.

`services/inputd/nix/input-seat-rules.nix` now applies the existing seat policy to `event*|js*`. Both interfaces use root ownership, the gameplay group, mode `0660`, and removal of `uaccess`. Exact canonical P1–P255 identities remain required. No broad input-group membership is added.

This closes the sibling-interface gap. It does not correct Dr. Mario's fallback assignment or prove that unrelated controller transports cannot reach a game.

## Verification

The expanded `unified-controller-input-vm` uses the production receiver, exact production seat rules, real kernel devices, and prebuilt SDL libraries. No host input device enters the VM.

| Check | Result |
| --- | --- |
| SDL2-compat 2.32.58 and SDL3 3.2.26, under gameplay UID/GID without extra groups | Four recognized neutral gamepads with zero sources; paths follow P1–P4 and player indices are 0–3. |
| Semantic input | All 15 buttons and six axes pass through the real gamepad APIs; P1 and P2 route independently. |
| Source loss, reconnect, leave/return, end | Four handles retain their paths, instance IDs, and player indices. Disconnect clears input; reconnect reclaims P1; leave/return requires neutral before rearm. |
| Linux joystick access before the rule change | Fails because an unrelated user can open a seat node. |
| Linux joystick access after the rule change | Passes for all four seats. Gameplay access remains; unrelated access is denied. |
| Existing receiver/kernel assertions | Pass, including six-seat mixed physical/remote allocation, authority checks, resizing, restart reconciliation, and shutdown. |
| Production host module and seat-core VM | Pass, including udev rule verification, exact identities above four, plugin-independent seat lifetime, browser denial, and receiver shutdown. |

Run the receiver/kernel/SDL proof with:

```sh
nix build .#checks.x86_64-linux.unified-controller-input-vm -L
```

These results cover the isolated receiver and gamepad APIs, not the complete korrid game sandbox, physical InputPlumber capture, other SDL versions, or arbitrary application assignment. The physical device was not changed during these checks. A core rollout needs separate approval. Preserve the current game and the account pin until an approved validation requires changing them.

A scoped per-boot rollout is prepared in `/tmp/controller-seat-policy-run.py`. After device approval, run `/tmp/controller-seat-policy-run.py apply` on the workstation. It copies a locally authored script and uses the target's installed Python. The script changes only the reviewed native udev rule, reloads rules, and triggers only validated seat nodes. It checks unchanged seat identities, input-service PIDs, session ID, and download-only policy. It does not restart services, launch games, alter trust, or inject input. The rule lasts until reboot; permanent deployment still needs the normal signed system-generation delivery.
