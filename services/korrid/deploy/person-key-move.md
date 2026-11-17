# Move one device off its kept person key

This is a one-off operator procedure. Do it once on each device that still
has a person key on disk. No code does it for you, and no code reads the old
key after the change (ADR 0082).

## Why

Before ADR 0082, the local signer kept a person key on each device in
`/var/lib/korri-local-signer/identity/active/person.key` (64 hex characters
and a newline). korrid bound that key as the device's owner at start.

Now no device keeps the person key. After the change is deployed:

- The local signer never reads `/var/lib/korri-local-signer`. The files stay
  on disk, but systemd no longer manages that directory.
- The device stays owned by the old person key.
- `korri identity switch words` and `korri join enter` cannot move the
  device by themselves. Each needs the old key to sign the removal of the
  device's old owner statement, and the old key has no recovery words and no
  NIP-46 signer.

This procedure lets the old key sign that one removal through a temporary
NIP-46 signer on the device. The device then moves to the person key that
the person's recovery words make.

## Before you start

- The device runs a korrid with recovery words (`korri identity switch
  words --help` shows `--current-owner-bunker`).
- You have a root shell on the device and the device's terminal, or SSH with
  a terminal.
- The person has 12 recovery words on paper. If the person has none yet, get
  new words from `korri setup finish` on a device in setup. Any device in
  setup works, also a test korrid that you start only for this, with its own
  state directory, local signer and CLI socket; delete that state
  afterwards. Every device that the person moves with these words then has
  the same person key.
- Find every device that names the old person key. `korri identity show`
  prints `Person key: …` on each. A device that joined this one names the
  same key. Move all of them while this device still has the old key file.

## Steps

Run the steps on the device, as root, while korrid runs.

1. Note the old person key:

   ```sh
   korri identity show
   ```

   Keep the `Person key:` line. Step 6 checks that it changed.

2. Start a relay that only this device can reach, in a second root shell:

   ```sh
   mkdir -m 0700 -p /run/korri-person-key-move/db
   cat >/run/korri-person-key-move/strfry.conf <<'EOF'
   db = "/run/korri-person-key-move/db/"
   relay {
     bind = "127.0.0.1"
     port = 10547
     nofiles = 0
   }
   EOF
   nix run nixpkgs#strfry -- --config=/run/korri-person-key-move/strfry.conf relay
   ```

   korrid accepts `ws://` only for a loopback relay, so nothing of the move
   leaves the device. korrid reads a NIP-46 reply from what the relay keeps.
   strfry keeps such replies for a while. `nak serve` keeps none, and the
   move then fails with `remote signer request timed out`. To move another
   device that names the same key, use a `wss://` relay that both devices
   reach instead (not tested).

3. Start a NIP-46 signer with the old key, in a third root shell:

   ```sh
   NOSTR_SECRET_KEY="$(cat /var/lib/korri-local-signer/identity/active/person.key)" \
     nix run nixpkgs#nak -- bunker ws://127.0.0.1:10547
   ```

   The key goes to nak in its environment, not in an argument: other users
   of the device can read arguments. nak prints a line `bunker:
   bunker://…?relay=…&secret=…`. Copy that URI. nak also prints the old key
   in its `to restart:` line, so close this shell when you are done.

4. Move the device:

   ```sh
   korri identity switch words --current-owner-bunker
   ```

   - At `Existing person data`, type `transfer`. The device keeps the old
     key's play log and the plugin secrets for the new person key.
   - At the trust prompt, type `confirm`. Pairings and stream trust of this
     device end.
   - At `The current owner's bunker URI`, paste the URI from step 3. The
     terminal does not show it.
   - At `The 12 recovery words of the new person key`, type the words from
     the paper, with spaces. The terminal does not show them.
   - Confirm with `y`.

   korrid asks the signer from step 3 to sign the removal of the old owner
   statement, and the local signer signs the new one with the words' key.
   The CLI prints `Device owner: …. Korri will restart to use the new
   identity.` korrid then restarts with a new device key.

5. Stop the signer and the relay with Ctrl-C in their shells. Close the
   signer's shell. Remove the relay's files:

   ```sh
   rm -r /run/korri-person-key-move
   ```

6. Check the result:

   ```sh
   korri identity show
   ```

   The `Person key:` differs from step 1. It is the same on every device
   that the person moved with the same words.

7. After the last device that named the old key has moved, delete the old
   key:

   ```sh
   rm -r /var/lib/korri-local-signer
   ```

   This cannot be undone. Do it only after step 6 passed on each device.

## If a step fails

- korrid restarts after every switch attempt, also a failed one. Wait until
  `korri identity show` answers before you try again.
- The switch stops before it changes anything when a signature fails. The
  device stays owned by the old key. Correct the cause and run step 4 again.
- `The replacement identity must use a different person key`: the words
  make the old key. Nothing changed.
- `An identity switch is already pending recovery`: restart korrid
  (`systemctl restart korrid`). It finishes or undoes the pending switch at
  start. Then run `korri identity show`.
- A device that has a NIP-46 signer as its owner does not need this
  procedure: `korri identity switch words` asks that signer.

## How this was tested

The procedure ran on test korrids on aka, never on a real device: a device
state that the build before ADR 0082 made, with its `person.key`, then the
new build on that state, then steps 2 to 6 with nak 0.20.6 and strfry 1.0.4
(the versions of the Korri flake's nixpkgs). The device ended owned by the
person key of the words.
