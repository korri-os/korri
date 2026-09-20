# Device cache and build policy

Korri devices download prebuilt software. They must not compile software or
start remote builds. CI and build machines produce outputs instead. Runtime
TypeScript/JavaScript transpilation still follows `services/korrid/SCRIPTING.md`.

## Configuration

`nixosModules.korri-device-cache` supplies the Linux device policy. Its settings
come from Nix's configuration contract. The generic plugin-host module imports
it. Other device integrations and activation need their own approval.

| Setting | Effect |
|---|---|
| `max-jobs = 0` | Nix cannot schedule local builds with this configuration. |
| Empty `builders` and disabled distributed builds | The device cannot dispatch remote builds with this configuration. |
| `fallback = false` | A failed download does not request a source build. |
| `require-sigs = true` | Input-addressed outputs need a trusted signature. Content-addressed outputs are checked by their content address. |
| `always-allow-substitutes = true` | Devices can download outputs whose derivations normally prohibit substitution, including small wrappers. |

The module preserves the stock NixOS cache and key. It adds no third-party cache
or signing key. It forces the build prohibition and signature requirement even
when another module sets conflicting values. CI and development builders do not
import this device policy.

These settings are an operational restriction, not a security boundary.
Privileged users can change them or run a compiler directly. Product code and
deployment procedures must not bypass them. Never use `--no-check-sigs` or enable
builds to bypass a failed installation.

## Plugin downloads are not a general binary cache

The former Garnix configuration and trust key have been removed. No account or
GitHub app settings were changed. General core binary cache replacement remains
unresolved. The stock cache does not contain Korri's custom outputs. Do not
activate a device generation until every required output has a verified
prebuilt delivery route.

Production plugin packaging and the publication workflow belong to
[korri-os/plugins](https://github.com/korri-os/plugins). Core retains the generic
Rust host, publisher, and real cold-host tests. First-party publication uses
standard signed Nix file caches hosted by GitHub Releases. A mutable cache
release holds `nix-cache-info` and signed `.narinfo` files; immutable build
batches hold the compressed NAR payloads. Dependencies already verified in
trusted upstream caches are not uploaded again. See that repository's
`PUBLICATION.md` for the producer and core's plugin-host README for the importer.

Only a URL that serves the Nix cache protocol is a substituter. A generic
GitHub release page or an archive download URL is not. The raw-cache installer
combines the bound publisher cache with configured trusted upstream caches,
requires signatures, and refuses local or remote builds. The separate catalog
installer explicitly verifies and stages its archive before using the same
importer; it is not an automatic fallback for a failed raw-cache download.

Plugin publication does not make `korrid`, `korri-bundle`, or other core
binaries into plugins. Only an output with an actual plugin declaration is
eligible. The streaming host is no longer named here as a core binary: it is
planned as a bundled, removable plugin. Publication still does not make it one.
It becomes eligible when it ships a plugin declaration. Core's Tailscale fixture remains test-only; first-party game and
SSH outputs can be re-exported by the plugin repository without copying their
sources. Device-generation delivery still requires its own verified prebuilt
route.

## Device integration and cache failure

Build the device generation on a build machine. Activation and physical-device
acceptance require their existing separate approval. Use an exact output path
for the target architecture and verify that it is absent from the target store
before testing a real substitute download. An already-present path is not proof
of download. Inspect effective Nix settings, fetch the exact prebuilt path from
a configured cache, and run the downloaded executable.

Request an uncached derivation too. Nix must refuse it without starting a local
or remote builder. Keep the previous working software and its GC root selected
after any cache failure. Repair delivery on a build machine, never by enabling
compilation on a device. An exact output-path fetch does not evaluate nixpkgs;
`nix build .#korrid` does and is not a device installation command.

## Local verification

Run `nix build .#checks.x86_64-linux.korri-device-cache` on a build machine.
The check evaluates the module and runs locked Nix against a real local HTTP
cache, isolated temporary store, and temporary signing key. It makes no external
network calls during the test.

It proves signature refusal, signed downloads into an empty store, substitution
of small outputs, and cache-miss refusal with both values of `preferLocalBuild`.
It also checks unchanged stock cache/key settings, unconfigured builders, and
conflicting device settings. It does not prove a live publication service,
retention, or a physical ARM download. Missing outputs and outages can still
stop an update.
