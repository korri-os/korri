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
Rust host, publisher, and real cold-host tests. Plugin delivery uses complete
content-addressed Nix file-cache archives for Releases and a small, separately
hosted HTTPS catalog. It does not publish `korrid`, `korri-bundle`, Sunshine, or
other core binaries as plugins. Only a package with the actual declaration is
eligible; core's Tailscale fixture is test-only.

GitHub Releases serves files, not a standard Nix substituter directory. Do not
put a Release URL in `nix.settings.substituters`. The repository installer checks
the archive, stages a private local cache, then uses the existing verified Nix
importer. The explicit raw-cache CLI remains available for a separately
configured real Nix cache. Neither route may build on a device.

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
