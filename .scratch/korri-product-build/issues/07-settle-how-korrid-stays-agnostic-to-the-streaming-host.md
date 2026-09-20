# Settle how korrid stays agnostic to the streaming host

Status: ready-for-human
Blocked by: None

## What to build

Choose the clean boundary that lets korrid provision a stream and manage input without knowing Sunshine-specific names or protocols. Decide who owns the seat receiver and group before the streaming-host cut starts.

## Acceptance criteria

- [ ] Current source evidence is recorded for the certificate-control socket and protocol, the input-seat receiver and group, and the effects korrid performs today.
- [ ] The user chooses how certificate provisioning stops being Sunshine-specific while korrid still owns the effect.
- [ ] The user chooses whether the streaming plugin ships its seat receiver and group under its approval or korrid provides the first-party seat mechanism.
- [ ] The decision keeps plugin declarations effect-free and keeps native configuration in native artifacts.
- [ ] The decision names the security and lifecycle cost of the selected boundary, including removal and approval behavior.
- [ ] The durable answer is linked from this ticket and is sufficient for ticket 15 to implement without another architecture choice.
- [ ] No generic capability name, protocol, option, schema, compatibility path, or code change is invented before the user makes the two choices.
