---
status: accepted
---

# Declared games can come from their own package

**Owner decisions, 2026-10-20.** Issue #99 option 1: a declared game's release
can come from a file already shipped inside its own plugin package. The first
case is the Starter pack's 27 games. A device can be offline during setup.

- **A third `file` form.** Alongside ADR 0013's account-root string and artifact
  record, a release accepts `{ packageFile: "<manifest files key>", path:
  "<relative path inside that file or directory>" }`. The value's form says
  which folder the path starts from. No string is parsed. Unknown fields,
  unsafe paths and a non-empty `artifacts` list are refused.
- **Own package only.** `packageFile` must name a key in the declaring plugin's
  installed manifest `files`, as runner `installed_file` already requires.
  It cannot reach another plugin's files. `contentPath` is that installed
  path joined with `path`. It is ready whenever the plugin is enabled.
- **Why `packageFile`.** `file` repeats the outer field. `package` clashes with
  manifest `packages`, which uses the same `cartridges` key for a different
  path. `packageFile` names the `files` key, not the package output.
- **Content identity does not change.** Hashes are hashes of the content file.
  The pack's build check proves them; korrid does not hash package files at
  runtime. A copy found in a paired source joins by hash (ADR 0003).
- **No saved location.** Reconcile uses ADR 0013's existing library item,
  release keyed by its first hash, and provider link. There is no download
  and no Confirm. Discovery never reads this path under an account root.
- **Disabled means unavailable, not removed.** Disabling removes the plugin's
  declaration. Its releases stay in library history as unavailable, following
  ADR 0013's rule for dropped versions. No removal or migration is added.
- **Ship disabled; offer once in setup.** Product images keep packages and
  approved receipts, but seed `@korri:starter-pack` Disabled. FAKE-08, Solarus
  and ZQuest Classic stay Enabled. First-boot onboarding offers Install;
  Install enables the local pack, then rescans, without a network or restart.
  Installed is remembered only in memory for that setup session. Existing
  devices keep their current enabled pack; no runtime migration is needed.

## Consequences

Packages and approved receipts take space before the person accepts the offer.
The core trusts content hashes proven by the package build, rather than reading
and hashing every cartridge at runtime. Published pack pins must adopt these
declarations before their games can appear through this path.
