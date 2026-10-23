# RG35XX SP: repair the stranded identity-switch saves

This is an explicit offline operation, not an identity inference at daemon
startup. It requires the operator's consent and the existing private records.
Do not run it on the original backup or on a running installation.

## Grounding and cause

The owner's **2026-10-22** decision, ADR 0023, **“An owned device joins by
identity switch, keeping its data”**, says a device with an automatic identity
keeps its saves and play history when it joins. The decision is recorded in
`docs/device-join-design` at `b9d1c77a2` (not yet landed on main). This repair
uses that decision; it does not merge that branch. ADR 0014 requires the
previous real person's saves to stay theirs, and an offered import for an
automatic identity. Therefore this repair is never an automatic fallback from
a missing owner directory to whichever directory happens to exist.

Verified records from `backup-20261023T164148Z`, with UTC times:

- `f7a1bc9e47b9ed7cd2022d0046b65e03126888eb42d321f605c777af3419d5a9`
  is a **person key held by the local signer**, not a device key or NIP-46
  client key. The runtime public projection names it. The signer's private
  `identity/active/` is empty and its only retired record is
  `identity/retired/f7a1…/person.key`. The directory name is the signer's
  validated public-key index (`local_signer.rs::verify_retired_key`).
- The signer identity directory dates from first boot, 16:28:12.
  `local_signer.rs::initialize_keyring` generates the first person key;
  `identity.rs::bind_automatic_owner` binds it through the signer. This is
  consistent with the SP's automatic identity. The keyring does **not** store
  an automatic-versus-imported provenance marker. The repair does not treat
  the mere existence of a retired key as proof of permission to adopt saves.
- At 18:30:42, a 41-second Anguna session ended under the old device key
  `7fa858c5d4cf9cb380f70825a5f2ac1793f7e356c28b0abf7a5a19815bf5fe9f`.
  Snapshot `d61113264701fc2c9272f476e5889755771816d8a07d303732e9a7bc62f1c95c`
  remains in private `saves/f7a1…/anguna/`. Its two objects match the live
  32,768-byte SRAM and 37,063-byte savestate in `users/f7a1…/saves/anguna/`.
- The owner event created at 23:12:30 binds new device key
  `306cdc659700fb5510147aad9849d3abef6eb6d966977dbc55e77e84aafbc8e0` to
  `a80db13fd36d6e15cb45df12db658da04dd6dc7c6d9200dcebabdcb207ed3dca`.
  NIP-46 `userPublicKey` agrees. The same Anguna session is now in
  `play-log/a80d…/anguna.json`, written at 23:13:06.105. The old signer key was
  retired at 23:13:06.151. The switch journal is empty.

The shipped `identity_switch/coordinator.rs::switch` stages a new device key
and NIP-46 owner statement, signs the old revocation, and exchanges identity
directories. `complete_commit` transfers the play log, retires the active
local key for a remote replacement, resets trust, and removes the journal.
It never moves the live person directory or the private save history. That
omission explains the exact observed split. The old play log's `userId` is
rewritten by `host/play_log.rs::transfer_person_records`; snapshot records do
not contain a person field. Their owning person is the directory used by
`saves/store.rs::person` / `game`. Moving that complete directory preserves
snapshot IDs, parents, devices, releases, times, and shared object references.
`launcher/linux_plugin.rs` uses the owner-specific `saves::account_root`.
Neither it nor the save store looks in the retired person's tree for a80d.

Source verification on the build host:

- The Anguna session journal names `nh0a6sp1n6cw2wk8rhm7bsk1qjc20br4-korrid-0.0.0`.
  Its derivation's source wrapper points to
  `/nix/store/rl72g9yj401yspxvfbk1bwzcgd01han8-source`.
- Boot system `lm2hi53…` is not the whole runtime identity: the mutable active
  bundle `pcl5pv4…` resolves to `jnwchvk2kqs36hclxwciv771nbdxi89m-korrid-0.0.0`,
  also verified as the process executable since 23:13:08. Its derivation's
  source wrapper points to `/nix/store/y99ghpanqnma9i1r0mi21f5gj7kvzdw0-source`.
  Its `identity_switch/coordinator.rs`, `identity.rs`, and `local_signer.rs`
  match main 5097e69ea byte-for-byte. The earlier source also transfers only
  the play log. No Git SHA is embedded in the observed runtime receipts.
- The installed mGBA receipt points to
  `/nix/store/92c82hj672is3k2m72yvvihjr2jwwn4p-korri-plugin`.
  The real layout is already per-game. Do not run a flat-layout conversion
  against f7a1 or create an empty a80d tree to bypass the refusal.

## Stale runtime public key is not an active key

The old `/run/korri-local-signer/public/person.pub` names f7a1 and dates from
22:25:01, before the switch. In both shipped and main `local_signer.rs`,
`Activate` retires the active key, but publishes a new projection **only**
when the replacement is local. It does not remove the projection for a remote
replacement. `bin/korri-local-signer.rs` also publishes only an active key on
startup; it does not clear an old projection when the keyring is remote.
Thus a service restart alone does not repair the stale file. The empty
persistent `active/`, not this projection, establishes that f7a1 is retired.
Signer requests load active keys from the keyring, not from `person.pub`.

Main's `bind_automatic_owner` returns an existing Owned statement unchanged.
It will not generate or promote f7a1. The signer credential helper in
`nixos-module.nix` reads the current **device** identity (306cdc…), not the
person projection. After the steps below a80d is the only device owner and
NIP-46 signing person. The f7a1 private key remains **retired**, intentionally:
this operation neither deletes a person's key nor claims that a80d is the
only key stored on disk. The local signer does not hold a80d's private key.

## Stopped-device operation

1. End sessions, let the old korrid finish recovery, and stop `korrid.service`,
   `korri-local-signer.service`, and `korri-local-signer.socket`. Keep all
   three stopped. Back up all three state roots while stopped. The account
   script checks the session and switch journals but cannot stop services.
2. Obtain explicit consent to repair this particular switch. Set:

   ```sh
   OLD_PERSON=f7a1bc9e47b9ed7cd2022d0046b65e03126888eb42d321f605c777af3419d5a9
   OWNER=a80db13fd36d6e15cb45df12db658da04dd6dc7c6d9200dcebabdcb207ed3dca
   ./migrate-account.py --storage-root /var/lib/korri \
     --private-state-root /var/lib/korrid \
     --local-signer-state-root /var/lib/korri-local-signer \
     --adopt-retired-person "$OLD_PERSON:$OWNER" --dry-run
   ```

   Review and retain stdout/stderr in a restricted operator report. Then run
   the identical command without `--dry-run`, retaining its report too.
   These are CLI arguments, not new persistent identity schema.
3. Consent must explicitly name the captured `f7a1…:a80d…` pair. The script
   pins both full public keys, owner event ID
   `c4503c1d27ae38484ec0880886c03109f58d0aeeab71744547141faf209fa97f`,
   and its creation time `2026-10-22T23:12:30Z` (`1792710750`). A different
   internally consistent switch is refused; it needs its own approved repair.
   The script also requires exactly that retired key, an empty active directory,
   the matching owned/NIP-46 person, and each old snapshot's session in the
   new person's transferred play log. It checks complete snapshot ancestry,
   object hashes/lengths, and exact live-save equality to the unique head.
   It rejects existing destination trees rather than merging, other retired
   keys, unfinished switches, missing proof, unsupported history files,
   missing objects, altered live saves, and splits needing review. Like the
   original account script it trusts previously validated private records;
   it does not read private key bytes or reverify owner signatures.
4. It renames the whole old person directory, including `retroarch.cfg`, and
   the whole private person save-history directory to a80d. Shared objects
   stay in place. Every account/history/object file is reported with its
   source, destination, byte count and SHA-256. No record is rewritten.
   Each directory move refuses replacement. An interrupted apply can leave
   either move complete; repeat the same dry run before continuing. No marker
   or second copy is written.
5. Run the existing RetroArch dry run and apply as in README/ADR 0014. Expect
   `total=2 moved=0 retained=2 unresolved=0 unclassified=0`. The ACL is still
   required for current daemon/runtime access.
6. While both services and the socket are stopped, clear only the stale
   **runtime public projection**, after verifying it names the retired key:

   ```sh
   active=/var/lib/korri-local-signer/identity/active
   projection=/run/korri-local-signer/public/person.pub
   if ! test -d "$active" || test -L "$active"; then
     echo 'Refusing cleanup: active directory is missing or unsafe' >&2
     exit 1
   fi
   if ! active_entry=$(find "$active" -mindepth 1 -print -quit); then
     echo 'Refusing cleanup: cannot inspect active directory' >&2
     exit 1
   fi
   if test -n "$active_entry"; then
     echo 'Refusing cleanup: local signer still has an active key' >&2
     exit 1
   fi
   test -n "${OLD_PERSON:-}" || exit 1
   if test -e "$projection" || test -L "$projection"; then
     test -f "$projection" && test ! -L "$projection" || exit 1
     projected_person=$(cat "$projection") || exit 1
     test "$projected_person" = "$OLD_PERSON" || exit 1
     rm -- "$projection" || exit 1
   fi
   ```

   Stop if these checks fail; do not start the upgrade on failure. Do not
   remove `identity/`, the retired key, owner event, or NIP-46 state. A full
   reboot clears `/run` too, but a new binary/unit or service restart alone
   does not. Clearing this disposable projection is an explicit operational
   step, not another persisted identity source or a runtime fallback.
7. Activate the new system/bundle/plugin together. Restart the signer device
   credential unit, signer socket/service, then korrid (or reboot). Verify
   `identity status` still names a80d and device 306cdc…, persistent `active/`
   is empty, f7a1 remains retired, and `person.pub` is absent. Verify both
   Anguna live files and the existing snapshot are under a80d with unchanged
   hashes. Launch Anguna, check progress, end it, and check save history.
   First contact with another device may need the normal first-sync review.

The general identity-switch coordinator fix is separate. This repair refuses
layouts other than the proven stranded-switch case; it is not a general
person-to-person history importer or a bypass of the first-sync review.
