# Mini V2 controller seat permission rollout

The approved per-boot permission rule is applied and verified on the Retroid Pocket Mini V2. Four persistent gamepads remain present. No game was stopped and no input service was restarted.

Source change: `7520c16af`, described in [Controller seat normalization](2026-10-03-controller-seat-normalization.md).

## Approval and scope

Ask `2b1905fe-8b69-4587-8d0a-227d1c700fcb` selected `apply-rule`. Approval covered the per-boot native udev rule and verification only. It did not authorize a system generation, game launch, account edit, controller remap, or firmware write.

The rollout extends the installed rule's `event*` matcher to `event*|js*`. All other directives, 255 canonical identities, native program paths, and the gameplay GID remain unchanged. The original Nix-store-backed rule remains untouched.

## Verified result

| Property | Result |
| --- | --- |
| Active runtime rule | `/run/udev/rules.d/99-zz-korri-input-seat.rules` contains the exact reviewed delta. |
| Seat nodes | Four event nodes and four joystick nodes retain their original inode, device number, and sysfs identity. |
| Permissions | All eight nodes are root-owned, GID 1000, mode `0660`. Joystick nodes previously had input GID 174 and mode `0664`. |
| Gameplay access | A real UID 1000/GID 1000 process, without supplementary groups, opens all eight nodes. |
| Portal access | A real `korri-portal` process, without supplementary groups, cannot open any of the eight nodes. |
| Seat receiver | PID 1100 remains active. |
| Inputd | PID 1670 remains active. |
| Current session | Launch ID `c4d5005df4edc23cc6176751853b465c` remains unchanged. This was the session observed before rollout, not the old handoff session. |
| Device cache policy | `max-jobs=0`, builders empty, `fallback=false`, and `require-sigs=true` remain set. No build or trust update ran. |

The successful apply was `proc_509c`. A separate fresh read, `proc_d06b`, confirmed the rule, all eight node permissions, both service PIDs, and the same session ID.

## Corrected rollout failures

The first attempt used the existing filename under `/run`. Udev gives the same-named `/etc` file priority, so the trial rule was ignored. Verification failed, the script removed its trial, and a read confirmed baseline permissions.

The second attempt used the distinct later-sorting filename. Node permission checks passed, but the access-check helper could not read its own root-only script. SCP inherited root's `0077` umask and left the script at mode `0700`. The script rolled back this attempt too.

The final attempt made only the credential-free helper script readable at mode `0755`. The gameplay and portal access checks then passed. Neither failed attempt issued session control or restarted services.

## Artifacts and limits

The workstation runner is `/tmp/controller-seat-policy-run.py`. It copies `/tmp/controller-seat-policy-device.py` and runs it with the target's existing prebuilt Python. A fresh read uses:

```sh
/tmp/controller-seat-policy-run.py status
```

The successful baseline backup is `/var/tmp/controller-seat-policy-kwgvywz6` on the device. It contains the original rule and node metadata. The runtime rule disappears at reboot. Permanent deployment still needs normal signed system-generation delivery.

This closes the sibling-interface permission gap. It does not change application player assignment, remove the Dr. Mario account pin, implement seat swaps, or establish isolation of every other controller transport. The separate off-device SDL checks verify normalized input and stable seats through the tested APIs, not arbitrary game policy.
