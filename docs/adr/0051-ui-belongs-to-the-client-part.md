---
status: accepted
---

# UI belongs to the client part; a device may run only the server part

**Owner decisions, 2026-11-08.** aka (x86_64, RX 7900 XT, an HDMI dummy plug,
no gamepad) ran the whole handheld composition. Its shell was what a phone saw
over Moonlight between games. A bundle update rolled back because inputd waits
for a local gamepad. The library listed the input validation gate.

## Decision

- A device's software has a **client part** and a **server part**
  (`CONTEXT.md`). UI is the client part's job. Remote devices bring their own
  client part, so a device that only serves content runs no client part.
- These names describe software on a device, not device roles. A handheld
  runs both parts. aka runs only the server part.
- The product composition is split into the two parts. A handheld imports
  both. A server-only device imports the server part alone. There is no
  switch that turns client pieces off inside one composition.
- The device's Nix configuration states which parts it runs. Korri does not
  guess it from attached hardware: aka's dummy plug looks like a screen.
- Between games, a server-only device streams nothing. The stream opens with
  the game a peer starts and ends with it.
- The sway compositor stays in the server part for now as the host Gamescope
  runs in. It draws nothing once the shell is gone. Gamescope that drives the
  output directly is a later, separate change.
- The first server-only device is aka.
- The client part holds the shell, korri-inputd (local controller capture,
  shell buttons and Guide-button actions), local hardware controls (power
  button, lid, volume keys, brightness, display idle and wake), the shell
  freezer, and the input validation content (the gate, Neverball,
  `workspace-next`).
- The server part holds korrid, the plugin host, the compositor, the
  controller-seat receiver for streamed controllers, limitsd, the product
  systemd with its stop-thaw patch (korrid also freezes game units), and the rest of
  the product composition, including the USB-C gadget network, boot splash and
  kernel pin.
- Each part brings its own update checks. On a server-only device, an update
  has worked when korrid is healthy and every plugin that was on is running.
  The checks for korri-inputd readiness and "the shell shows a surface" belong
  to the client part. This amends ADR 0050 for devices without a client part.

- The flake keeps `nixosModules.korri-product` as both parts and adds
  `nixosModules.korri-server` for the server part alone. aka imports
  `korri-server`. A client-only module waits for a device that needs one.
- Done means: aka runs no shell, korri-inputd, freezer or controls units and
  lists no validation games; a bundle update on aka succeeds with no gamepad
  and no rollback; the owner's phone starts a game on aka, streams it with a
  controller, and sees nothing from aka between games. Linux handhelds wait
  until aka and the phone pass. Their check that the split leaves their
  systems unchanged comes after that.

## Consequences

- A controller plugged straight into a server-only device does nothing.
  Controllers reach its games only through a peer's stream.
- A bundle switch no longer waits for a local gamepad on aka.
