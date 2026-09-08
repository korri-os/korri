# Device cache and build policy

Korri devices download prebuilt software. They must not compile software or
start remote builds. CI and build machines produce the outputs instead.

Plugin logic remains TypeScript or JavaScript source. Its in-process
transpilation and evaluation follow `services/korrid/SCRIPTING.md`. This policy
prohibits software builds during device installation and updates. It does not
change the plugin runtime.

## Configuration

`nixosModules.korri-device-cache` supplies the Linux device policy. Its settings
come from Nix's configuration contract and [Garnix's cache documentation](https://garnix.io/docs/ci/caching/).
The existing Linux images do not import this module yet. Device integration and
activation are separate work. Do not activate the policy before the required
outputs are available in the cache.

| Setting | Effect |
|---|---|
| `max-jobs = 0` | Nix cannot schedule local builds with this configuration. |
| Empty `builders` and disabled distributed builds | The device cannot dispatch a build to another machine with this configuration. |
| `fallback = false` | A failed substitute download does not request a source build. |
| `require-sigs = true` | Nix requires a trusted signature for input-addressed cache outputs. Nix checks content-addressed outputs by their content address. |
| `always-allow-substitutes = true` | Nix can download outputs whose derivations normally prohibit substitution, including small wrappers. |

The module adds `https://cache.garnix.io` and its published public key. It keeps
the default NixOS cache and key. It forces the build prohibition and signature
requirement against conflicting NixOS module settings. Other Korri modules do
not import it, so CI and development builders remain unchanged.

These settings are an operational restriction, not a security boundary.
Clients can override settings where Nix permits it. A privileged user can
change the policy or run a compiler directly. Korri code and deployment
procedures must never bypass the restriction on a target device.
Do not use `--no-check-sigs` or enable builds to bypass a failed installation.

## Build selection

The root `garnix.yaml` uses the provider's [documented configuration](https://garnix.io/docs/ci/yaml_config/).
It names Linux outputs explicitly. Both `x86_64-linux` and `aarch64-linux` have
core packages and module checks. The ARM list also contains the RKMPP and
V4L2M2M Sunshine packages.

The list includes `korri-bundle`, not only its component binaries. Devices need
the prebuilt directory that joins those components. It excludes kernels, SD
images, development shells, Darwin outputs, and the boot-splash VM test.
`korrid-linux-device-module` and `korri-linux-host-module` also stay outside
this list. Their evaluation reads generated helper files and requires builds.
The list includes `korri-tailscale` and its package check on both Linux
architectures. `plugins/tailscale/` owns that independently installed dependency.

An explicit list prevents new flake outputs from silently increasing CI work.
It does not limit the dependencies of a listed output. For example, korrid
includes emulator dependencies, and Sunshine has architecture-specific
libraries. A successful build must cache the full runtime dependency set.

## Enable Garnix

1. Publish the reviewed `garnix.yaml` before enabling the GitHub app.
2. Select the free plan and inspect the account's billing limits.
3. Install [garnix-ci](https://github.com/apps/garnix-ci) for `simonwjackson/korri` only.
4. Push a reviewed commit to start a build.
5. Inspect the selected outputs and their logs in Garnix.
6. Wait for the required ARM outputs to succeed before device integration.

Do not enable hosting, accept paid terms, or grant access to other repositories
as part of this setup. The [current pricing page](https://garnix.io/pricing)
lists 1,500 free minutes per month. It also lists additional minutes at
$0.006 per minute. The repository cannot enforce the account's spending limit.
Extra [open-source support](https://garnix.io/docs/open_source/) requires a
request to Garnix. It is not an unlimited free-build entitlement.

## Device integration and acceptance

Build the device generation on a build machine with
`korri.nixosModules.korri-device-cache` in its imports. Activation requires the
existing device approval procedure. This change does not activate or flash a
device.

For acceptance, use an exact aarch64 korrid output path from a successful
Garnix build. Make sure that the path is absent from the target store. Check the
effective Nix settings, then use `nix-store --realise` with that exact path.
Inspect the download log and run the downloaded executable. A path already in
the store does not prove substitution.

Also request an uncached derivation. Nix must refuse it without starting a
builder. Keep the previous working software selected after a cache failure.
Do not remove its garbage-collection root to make space for an unverified
replacement.

An exact output-path fetch does not evaluate nixpkgs. A request such as
`nix build .#korrid` still evaluates the flake. A cache stores Nix archives and
metadata, not ZIP files. Offline archive delivery and plugin installation are
separate work. The [Linux plugin installation brief](../../docs/briefs/2026-09-08-linux-plugin-installation-brief.md)
records the owner's trust decision.

## Local verification

Run `nix build .#checks.x86_64-linux.korri-device-cache` on a build machine.
The check evaluates the module and executes the locked Nix package against a
real local HTTP cache. It uses an isolated temporary store and a temporary
signing key. It makes no external network calls during the test.

The check proves signature refusal, signed downloads into an empty store,
substitution of small outputs, and cache-miss refusal with both values of
`preferLocalBuild`. It also proves that the module leaves unconfigured builders
unchanged and overrides conflicting device build settings.

This check does not prove Garnix account access, cache retention, or a physical
ARM device download. Missing cache outputs, cache outages, or key changes can
still stop an update. Repair the cache or publish a corrected output from a
build machine. Do not repair the failure by compiling on the device.
