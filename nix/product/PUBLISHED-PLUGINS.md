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

The pins come from immutable publisher batches `build-e5e27ed406f2`,
`build-9f946f33bd18` and `build-0febdc65ce51`. Each batch supplies `revision.txt`, architecture-specific
path lists, and `offline-metadata-SYSTEM.tar.gz`. Current host seed reports
establish the declaration IDs of the selected packages. Path names and lists
alone establish no identity or trust.

Both architecture maps contain 23 outputs. Batch `build-9f946f33bd18` supplies
Sunshine and the unchanged SSH output. Batch `build-0febdc65ce51` supplies
`@korri:starter-pack` and its exact `@korri:fake08` dependency from source
`0febdc65ce511bb3bf2862002839a9f2873ce50f`. The other 21 output pins are unchanged.
`published-plugin-metadata.nix` pins the
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

All seven product devices select Starter pack and FAKE-08 by default:
`rg353m`, `rgds`, `r36tmax`, `rpminiv2`, `odin2portal`, `rg35xxsp` and `rg35xxpro`.
Their product images register offline proofs before plugin recovery.
The pack retains 24 games, 25 original cartridges, credits, notices and
noncommercial CC BY-NC-SA 4.0. Installation does not add library entries or
game tiles. Into Ruins remains a known FAKE-08 failure.
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
korrid. It also checks the pack's exact required runtime, tampered graph approval
refusal, required-runtime disable refusal and dependency-aware re-enable.
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
