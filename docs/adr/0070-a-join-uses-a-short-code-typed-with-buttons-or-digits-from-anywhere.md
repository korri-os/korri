# A join uses a short code, typed with buttons or digits, from anywhere

**Owner decision, 2026-11-16.** Design in progress: the key exchange is not
chosen.

An owned device shows a join confirmation code. The person enters it on the
new device. The two devices can be on any networks, near or far. They find
each other through public Nostr relays (ADR 0026).

The code is 10 symbols. Each symbol is a digit from 1 to 6, and each digit
has one button: Up, Right, Down, Left, confirm and back. The screen shows
every symbol as both its digit and its button. A person with a controller
presses the buttons. A person with a keyboard, or at a terminal, types the
digits.

The first 3 symbols are a public nameplate. They tell the new device where on
the relays the owned device waits. The owned device picks a nameplate that no
other join uses at that moment. The last 7 symbols are the secret: 279,936
possible values. The meeting place never comes from the secret, because a
relay could try every value at once and learn it (owner decision,
2026-11-16).

Every Korri controller has a D-pad and two face buttons, and nothing more is
certain. Start and Select often open a system menu, so the code does not use
them. Face buttons go by their role (confirm, back), not by letter, because
Nintendo and Xbox swap A and B.

The two devices run a key exchange built on the code (a PAKE). A relay, or
anyone who reads the relay, cannot get between the two devices without the
code. Each code allows one guess.

A code is valid for 5 minutes. When it runs out, the owned device shows a new
one. A wrong code ends that code at once: the owned device shows a new code
and says that a wrong code was tried (owner decisions, 2026-11-16).

When the code matches, the owned device asks for one Accept. It shows the
joining device's name, what kind of device it is, and how it connects. A name
can be faked, so the name alone is not enough (owner decision, 2026-11-16).
The person can refuse a device they did not expect.

Korri never trusts a network. A home network, an airport Wi-Fi and a tailnet
are the same to a join: a nearby path only helps the devices find each other
faster. Only the code proves which device is joining. Korri never needs
Tailscale (ADR 0026); it is an optional plugin (ADR 0041).

A device with no screen joins with the `korri` command on that device, from a
terminal or SSH.

## Considered options

- **Both screens show the same code, and the person compares it.** It needs a
  separate way to find the other device when far. Replaced by the typed code,
  which finds the device and proves it in one step.
- **QR code.** Rejected: many Korri devices have no camera.
- **Accept with no code.** Rejected: another device could race the real one.
- **Same home network or tailnet only.** Rejected by the owner: he wants to
  join a device from anywhere.
- **Hex (16 buttons) or 8 buttons.** Shorter codes. Rejected: many devices lack
  shoulder buttons or stick clicks, and Start and Select open menus.

## Consequences

- The 8-digit join code of the earlier device-join design is replaced.
- A join can fail when relays are down or blocked. Korri says so clearly.
