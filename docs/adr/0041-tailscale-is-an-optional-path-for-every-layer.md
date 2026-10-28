# Tailscale is an optional path for every layer

**Owner decision, 2026-10-28.** This changes ADR 0026 for devices that have
Tailscale.

ADR 0026 says Korri uses Tailscale as one more method when a device already
has it, and never requires it. It also says no third-party relay carries
traffic, including Tailscale's DERP. korrid followed both rules by using
Tailscale addresses for peer `/rpc` and by removing them from every iroh
connection. So netplay and the game copy never used Tailscale, even when both
devices had it.

The owner decided: "I will never tightly couple tailscale to korri, but I will
add it as an optional plugin. I would want tailscale to passively be an option
for all layers."

## Decision

- Tailscale stays optional. Korri never requires it, never installs it by
  itself, and never depends on a Tailscale account. The `@korri:tailscale`
  plugin provides it.
- When a device has Tailscale up, its Tailscale addresses are ordinary path
  candidates on every layer: peer `/rpc`, the iroh connection that carries
  stream play, netplay and the game copy, and a federation device's relay.
- Korri does not prefer or force Tailscale. Direct paths and federation relays
  work as before, and Tailscale is one more candidate among them.
- Korri does not run Tailscale's DERP servers or talk to them. Tailscale itself
  may carry a Tailscale path through DERP. A person who installs the Tailscale
  plugin accepts that.

## Considered options

- **Keep Tailscale out of iroh.** Rejected: with Tailscale on both devices,
  netplay could still fail with "No federation route" while a working path
  existed.
- **Prefer Tailscale whenever it is up.** Rejected: it would couple Korri's
  behaviour to an outside service.

## Consequences

- ADR 0026's "no third-party relay" rule now holds only for devices without
  Tailscale. Korri itself still runs and uses no third-party relay.
- With Tailscale, encrypted iroh traffic can travel inside Tailscale's
  encryption, so it is encrypted twice.
- `services/korrid/src/federation/reach/` must stop removing Tailscale
  addresses from iroh candidates and relay listeners.
