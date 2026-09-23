# Declare sleep states, handle power and lid, and freeze before suspend

Status: ready-for-agent
Blocked by: 04, Phase 2 gate

## What to build

Make power and lid controls honest on every device. A device uses a declared sleep state when it has one and shuts down cleanly when it has none, while preserving the live-session position whenever sleep really occurs.

## Acceptance criteria

- [ ] Before implementation, the user approves the Nix option name and location for sleep-state declarations and the product-wide light-sleep shutdown delay.
- [ ] The model uses only the approved state names: light sleep, deep sleep, and hibernation. A device declares every state it has, or none.
- [ ] All six current devices declare no sleep state on day one and record that limit explicitly. No declaration is inferred from ROCKNIX behavior or unverified kernel support.
- [ ] The preference order is deep sleep, then light sleep, then clean shutdown. Hibernation remains unimplemented.
- [ ] One power-button press enters the preferred available state. A second press returns. Lid close and open perform the same two actions.
- [ ] On a device with no sleep state, power press and lid close shut the device down cleanly without a prompt. The ignored-power-key behavior is removed.
- [ ] Before light or deep sleep, korrid freezes the exact live session, local or streamed.
- [ ] Wake thaws only when the player slept from the game. A player who slept from the gameplay overlay wakes to the overlay.
- [ ] Light sleep turns the screen off, keeps the device awake, and shuts down cleanly after the approved fixed delay.
- [ ] Deep sleep uses suspend to RAM and has no product time limit.
- [ ] Automated tests cover declaration evaluation, state preference, no-state shutdown, power and lid events, exact-session freeze, and wake destination.
- [ ] No test result is used to claim that a physical device can suspend. A wrong declaration remains ordinary maintenance under the approved policy.
- [ ] No hibernation storage, swap policy, compatibility path, or fallback behavior is added.

## Decisions, 2026-09-22

Simon approved `services.korriProduct.sleep.states` as the Nix declaration and 900 seconds as the product-wide light-sleep shutdown delay. All six devices still declare no state on day one. The delay must not be treated as an active timer until light sleep has a working freeze and wake handler. The worktree now declares no state on the five existing product configurations and replaces Odin's ignored physical controls with logind shutdown. It rejects nonempty declarations until exact-session freeze and wake are implemented. RG35XXSP remains outside the product module pending hardware facts. No sleep acceptance check has run.
