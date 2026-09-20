# Settle how korrid stays agnostic to the streaming host

Status: resolved
Blocked by: None

## What to build

Choose the clean boundary that lets korrid provision a stream and manage input without knowing Sunshine-specific names or protocols. Decide who owns the seat receiver and group before the streaming-host cut starts.

## Acceptance criteria

- [x] Current source evidence is recorded for the certificate-control socket and protocol, the input-seat receiver and group, and the effects korrid performs today.
- [x] The user chooses how certificate provisioning stops being Sunshine-specific while korrid still owns the effect.
- [x] The user chooses whether the streaming plugin ships its seat receiver and group under its approval or korrid provides the first-party seat mechanism.
- [x] The decision keeps plugin declarations effect-free and keeps native configuration in native artifacts.
- [x] The decision names the security and lifecycle cost of the selected boundary, including removal and approval behavior.
- [x] The durable answer is linked from this ticket and is sufficient for ticket 15 to implement without another architecture choice.
- [x] No generic capability name, protocol, option, schema, compatibility path, or code change was invented before the user made the two choices.

## Answer

Resolved 2026-09-20 through two explicit user choices.

### Certificate provisioning

korrid owns a first-party GameStream/Moonlight trust effect. It keeps the bounded attest, provision, and revoke operations and the existing protected local socket contract. The streaming plugin ships the native host adapter and socket unit that implement that contract.

The cut removes Sunshine-specific knowledge from korrid's adapter, environment names, errors, paths, and service wiring. korrid knows the streaming trust protocol that the product uses. It does not know which host program implements it. The implementation does not add a generic plugin-effect registry or a second protocol.

The streaming plugin's native artifact keeps host-specific state handling. For Sunshine, that is the reviewed patch that atomically updates its existing trust state and verifies the live TLS reader before success. The plugin declaration stays effect-free.

Cost: korrid still owns one protocol-specific trust effect. A different streaming protocol needs a different first-party contract or an explicit later redesign. This choice avoids a speculative generic effect schema now.

### Input-seat ownership

The streaming plugin owns the input-seat receiver unit, dedicated service group, udev rules, mirror socket, and required device access. It requests each unit, directive, capability, group, and device rule through the exact approval path from ticket 02.

korrid keeps only its existing generic input-seat lease boundary. It sends start and stop for one exact launch ID through `InputSeatManager`. It holds no Sunshine-specific group, mirror path, packet format, or service name.

The plugin can package the existing prebuilt `korri-input-seat-receiver` program as one of its native artifacts. Native unit files own the service configuration. The TypeScript declaration only references those artifacts.

Disable, rollback, and removal stop the receiver and withdraw its group, udev rules, sockets, and device access with the rest of the plugin. Removal leaves no seat service or Sunshine-specific authority in the product module.

Cost: the streaming plugin approval is large. It includes a root receiver, dedicated group, `/dev/uinput` access, udev rules, and the capabilities needed by the reviewed receiver. Administrators must see and approve that authority for the exact plugin build.

### Implementation boundary

Ticket 15 implements this as one clean cut. It deletes the shared-host Sunshine socket, seat service, group, and udev composition in the same change that installs their plugin-owned replacements. It adds no compatibility unit, fallback socket, dual protocol, alias, or migration.
