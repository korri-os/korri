# Published plugins in installation images

Core updates do not recalculate plugin packages. Images select the exact
published outputs in `published-plugins.nix`, through the existing device
choices in `plugin-selection.nix`. No publisher flake or plugin recipe enters
this image dependency graph. Native Nix path contexts retain each complete
closure without a plugin derivation that can rebuild it.

Plugin approval still binds exact package permissions. Image assembly
uses the host `seed-graph` producer and `image-seed.sh` to create each
receipt and active GC root. The selection includes every exact required plugin;
closure retention alone does not install or approve that dependency. Offline recovery rechecks that approval and the
full key bound to the publisher namespace. Inclusion in the default bundle
does not make a plugin required.

## Current producer evidence

The pins come from the immutable publisher batches `build-9514b1b382d3` and
`build-9514b1b382d3-solarus`. Each batch supplies `revision.txt`, architecture-specific path lists, and
`offline-metadata-SYSTEM.tar.gz`. Current host seed reports establish the
declaration IDs of the selected packages. Path names and lists alone establish
no identity or trust.

Both architecture maps contain 32 outputs. The initial 24 outputs came from source
`9514b1b382d3b963674628308d6a5770987a4bc0`: the first batch republished 23
selected plugins for the discovery v2 plugin contract (system content rules,
groups and `controls`; no `discovery` export), which the current host requires.
The second batch adds `@korri:solarus`, which the Starter pack requires next to
`@korri:fake08`; `plugin-selection.nix` selects it on every product device.
Source `ac8fae593e42dda3cf4beda0b26f190a28d6b942` supplied batch
`build-ac8fae593e42-zquest`, which replaced the Starter pack (adding Lantern Hollow)
and added `@korri:zquest-classic`, which the pack now also requires; batch
`build-ac8fae593e42` added `@korri:gamescope`, selected on every product device.
Batch `build-dea5fd453fc6` (source `dea5fd453fc64d65edcdad1171cec85fb68be1f7`)
replaces it with the build that skips fully clipped planes and waits for the
host's configure after an unmap, so nested Gamescope no longer aborts when a
window moves off screen or is hidden and shown again.
Steam adds three outputs, selected on the RP Mini V2 only. `@korri:fex`,
`@korri:proton` and `@korri:steam` come from batch `build-365ce4e054d5` (source
`365ce4e054d56b0607a8d67a2bc42a0fc262e9e1`): the ADR 0012 builds, in which FEX
and Proton declare their pinned payloads as artifacts with unpack programs and
Steam lists both artifacts. Steam's manifest `requires` exactly those FEX and
Proton outputs of the same architecture. They replace the prepare-program
builds of batches `build-e479fd55b3aa` and `build-96a06b0a82ef`; with no pinned
output left in those batches, their offline archives are no longer pinned (the
metadata check builds and verifies both architectures without them).
Batch `build-75eb41b75907` (source
`75eb41b759074b92a4dac54e8ef6d302463d1ba0`) replaces only the Starter pack.
Its 27 declared games use `packageFile` releases from its own installed files
(ADR 0016). The public release's `revision.txt` matches that source, and its
`paths-SYSTEM.txt` assets name the pinned ARM and x86 outputs. FAKE-08,
Solarus and ZQuest Classic retain their existing exact pins. The matching
`offline-metadata-SYSTEM.tar.gz` archives are pinned with their SHA-256 hashes.
Batch `build-824050bfa4c3` (source `824050bfa4c3139ce3df9ee034a81e4f779e7fd9`,
publisher `main` `d6d0cf4` plus the three plugins and a Core input lock at
`bf1e59f3f`) adds `@korri:pico8`, `@korri:homebrewhub` and `@korri:retrobrews`,
selected Enabled on all seven product devices (owner decision 2026-10-21).
They are source-only packages (`plugin.ts` and the generated manifest, no
references); pico8's manifest declares the host `www.lexaloffle.com`. The
batch lists three paths per architecture. They were fetched into an empty store
from the publisher cache with only the `korri-plugins-1` key trusted and
verified, and its offline archives hold one signed record per output.
Batch `build-30594ae66c81` (source `30594ae66c81222f5d56a5e6d1b8c282c2b7fdf3`,
Core input `7f37bd48f`) republishes 23 outputs per architecture: the RetroArch
runner declaring per-game save locations under `saveRoot` (#126), its libretro
cores, FAKE-08, Solarus, ZQuest Classic and the Starter pack, whose fixtures now
run through `korrid plugin-run`. Each pin was evaluated at that source and found
in the batch's `paths-SYSTEM.txt`. Gamescope keeps its earlier pin.
Each pinned path is grounded in its batch's `paths-SYSTEM.txt` and
`revision.txt`. The new Starter pack pins were checked against those public
release assets directly. Earlier batches remain published even when their
plugin outputs are no longer selected. `published-plugin-metadata.nix` pins the
matching ARM and x86 archive hashes. It checks every path in the actual
closure, not only top-level packages. It rejects conflicting proof metadata.
Equivalent records can differ in optional `Deriver` provenance and signature
lines. The merger keeps the first record's provenance and combines signatures.
All other metadata must match exactly. This handles the three observed shared
proof differences per architecture in the new batch. Neither the optional
provenance nor a signature label grants trust.
A private Nix database starts without signatures, loads image-style store
registration, then imports only the pinned local proofs. Recursive Nix
verification checks contents and signatures with the bound full public key.
It uses no remote substituter and changes no build-host signature database.

All seven product devices ship Starter pack Disabled and its FAKE-08, Solarus
and ZQuest Classic runners Enabled:
`rg353m`, `rgds`, `r36tmax`, `rpminiv2`, `odin2portal`, `rg35xxsp` and `rg35xxpro`.
Their product images register offline proofs before plugin recovery.
The pack retains its games, original cartridges, credits, notices and
noncommercial CC BY-NC-SA 4.0. First-boot setup offers to install the pack;
installation means enabling its approved local package, with no network.
Declared `packageFile` releases then enter the library without saved locations
(ADR 0016). The current pins contain all 27 package-file game declarations.
Into Ruins remains a known FAKE-08 failure.
This change does not install software or write firmware on existing devices.
Recovery images keep their existing no-default-plugin behavior. R36T Max
retains its hardware-owned root-population command and now uses the same
proof prerequisite as the other product images.

## Build-host acceptance

Run from a Core checkout on a build host, never on a handheld:

```sh
nix run .#korri-published-plugins-check
```

The task adds the existing publisher cache and full key only to its process
configuration. It downloads exact paths with local and remote builds disabled.
A missing output or cache failure stops the task; it never evaluates a plugin
recipe instead. Current Core and the test harness can build on that host.

The task checks both architectures. It changes the real Core runtime source
and requires its derivation to change, while all published output paths,
proof derivations and host admission checks remain unchanged.
It checks each package ID through the current host. Proof tests remove or
alter a transitive dependency proof and test invalid signatures and a wrong
key against unsigned private databases. Archive tests use the actual pinned
release archives, including their shared-proofs differences. The current callback test uses the
unchanged published mGBA source graph, named native files and launch treaty.
A separate negative control changes only Core's `LAUNCH_PREPARE` operation
name and requires that same acceptance test to fail at the named-operation
interface.

On x86, the disposable VM tests offline image receipts, invalid approval
refusal, incomplete proof refusal, actual key-only SSH login, disable,
re-enable, reboot recovery, and the published mGBA registry consumed by
korrid. It also seeds the pack's exact required FAKE-08, Solarus and ZQuest
Classic runtimes. With networking blocked and mGBA disabled, enabling the
image-seeded Disabled pack and rescanning must list exactly its 27 declared
games through korrid. The VM also checks tampered graph approval refusal,
required-runtime disable refusal and dependency-aware re-enable.
It uses the production publisher key and the exact published packages. It
creates no test-signed replacement packages. ARM image proofs run on the
build host; native ARM lifecycle and handheld behavior remain separate tests.

The game test checks route construction and callback output. It does not
claim emulator gameplay with a valid ROM. Raw-cache `inspect` and `install`
can initialize their bound network cache even when package bytes are local.
That existing availability requirement remains unchanged. Offline image
installation uses seeded approved receipts and `restore-all`, not those
raw-cache acquisition commands.

`.github/workflows/published-plugin-checks.yml` runs this task for current
Core without publishing plugins. CI results do not qualify a supported image
or approve a device deployment.

## Updating a plugin selection

1. Obtain the actual published batch evidence and exact outputs on a build host.
2. Make sure that the current host reports each selected declaration ID.
3. Change the native output pins and matching offline archive hashes explicitly.
4. Run the complete acceptance task before image delivery.
5. Obtain separate approval for publication or handheld deployment.

Keep the publisher's Core input pin for unrelated changes. Adopt a new pin
only when a consumed builder, contract, native package or toolchain needs an
update. Compare actual output paths before selecting affected packages for
publication. Publisher `PUBLICATION.md` owns that operation.

Independent closures can retain different dependency versions and use more
disk. A Core dependency update does not update a plugin's dependencies.
Security updates to those dependencies still require explicit plugin updates.

An inherited image limit also remains: the boot proof service and its metadata
retain the default closures in the system generation. Uninstall removes a
plugin's selection and effects, but does not make those bytes collectible
while that generation remains. The earlier 21-output checks measured 445 x86
and 446 ARM direct references in the metadata outputs. Adding Starter pack and
FAKE-08 does not change that retention rule. This change does not discard
references or introduce a new proof-storage schema.
