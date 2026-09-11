# Device image builds

`.github/workflows/device-images.yml` builds the selected Linux SD image.
The `device` choice is `rg353m`, `odin2portal`, or `rgds`, with RG353M as the default.
All use their `packages.aarch64-linux.<device>-sd-image` output on
`ubuntu-24.04-arm`. No Android repacking, installer ISO, plugin integration,
or first-boot setup is performed.

RG DS is a console bring-up candidate, not an automatic portal session.
Its device-local Linux and U-Boot sources do not alter the other devices.
See [`../devices/rgds/README.md`](../devices/rgds/README.md) for SD-only
arrival checks, physical root-console access, and unverified hardware gates.

Odin first builds its existing x86 cross-compiled kernel, rescue kernel, firmware,
and static portal bundle in a separate `ubuntu-24.04` job. The kiosk already
selects that x86-built web bundle because its Bun dependency hash is platform-specific.
Only static web assets enter the ARM image. The job builds sequentially to
bound temporary disk use. That job exports every output and its closure to a
native signed Nix binary cache, then uploads the cache as an exact workflow
artifact. The ARM job checks the source revision and imports the paths derived
from its own checkout with signature checking enabled.

The signing key is generated per export. Only the public key enters the cache
artifact; the secret is removed with the temporary build directory. Trust in
this public key comes from the same workflow's exact artifact ID, not a general
publisher identity. It is supplied only to the CI import command and is never
added to device configuration. Cross-cache artifacts expire after one day.

## Run through GitHub Actions

1. Open **Actions → Device images → Run workflow**.
2. Select a branch and device. Leave `publish` off for a candidate build.
3. Download the workflow artifact containing the image, its `.sha256` file,
   and `korri-revision.txt`.

To publish an existing tag, dispatch through the GitHub CLI:

```sh
gh workflow run device-images.yml --ref <existing-device-tag> -f device=odin2portal -f publish=true
```

The workflow must already exist on GitHub's default branch, and the selected
tag must contain it. Use a distinct tag for each device release. The workflow
checks that the tag still names the built commit, then creates a GitHub prerelease.
It uploads into a draft before making the release visible. It never creates or
moves tags and never replaces existing releases or assets. After a failed upload,
inspect the draft and remove it explicitly before retrying publication.

Artifacts in a public repository are not private. Candidate artifacts expire
after seven days; published prereleases remain until explicitly removed.
The download step selects the upload action's exact artifact ID, including when
only the release job is retried.

## Access policy and verification scope

- SSH is disabled. No personal root SSH key is included.
- RG353M USB networking and serial remain enabled. Odin retains its panel
  recovery console. Physical consoles grant passwordless root access.
- Odin produces SD-card files only. Its existing AYN loader and Android install
  remain untouched. Firmware keeps its existing `unfreeRedistributableFirmware`
  classification; downloaded blobs are included in the resulting image.
- Builds use pure flake evaluation and unset `KORRI_WIFI_ENV`. Test-only WiFi
  fixtures used by the policy checks are not supplied to the image build.
- Staging checks compression, SHA256, and source revision. These are artifact
  checks, not filesystem, boot, input, networking, or gameplay acceptance.
- All published outputs are marked **unverified**. Hardware verification happens
  separately. The workflow never connects to or flashes a device.
- First-boot setup and plugin support are TBD. This workflow does not promise
  that the current image can later enable SSH through a plugin without an update.

## Runtime footprint

Production images omit `glmark2`, `mesa-demos`, `vulkan-tools`, and the RG353M
browser benchmark helpers. The layout check guards the package selections and
retained graphics/seat/kiosk configuration. Benchmark removal does not remove
Chromium, graphics drivers, or recovery kernels. RG353M firmware now follows the
[board/accessory family policy](../devices/rg353m/FIRMWARE.md); Odin firmware is unchanged.

RG353M private candidates measured these comparable `zstd -19 -T2` image sizes,
using the same kernel and SSH settings at each step:

| Change | Image bytes | Step reduction |
|---|---:|---:|
| Before the board firmware policy | 1,623,922,066 | |
| Board and accessory firmware families | 985,040,250 | 39.3% |
| Command-line V4L diagnostics | 938,109,208 | 4.8% |
| On-demand pinned Nixpkgs source | 900,591,314 | 4.0% |

Each candidate booted from the authorized SD with the previous generation kept
for rollback. Chromium, graphics drivers, Mesa, LLVM and the recovery kernel are
unchanged and are not size candidates without separate hardware acceptance.

Sunshine provenance retains its approved source hash, version, derivation, and
patch records, but not a reference to the full build-source checkout. Its Nix
package rejects output references to that source. The existing package and
runtime-settings checks verify the remaining approval contract.

## Nixpkgs source delivery on RG353M

The RG353M registry uses the exact fetcher attributes from `flake.lock`, including
revision and NAR hash, instead of a path to bundled Nixpkgs sources. The existing
`nixpkgs=flake:nixpkgs` NIX_PATH alias remains. An uncached lookup of that alias
needs network access, but retains the same revision and content pin.
Nix itself and the download-only device policy are unchanged.

The registry check verifies the emitted JSON and the native Nix reader. The
system builder rejects the Nixpkgs source path anywhere in its output closure.
Existing copies can remain on a device through rollback generations or a later
explicit source lookup; this change does not delete them or run garbage collection.

## Local commands

Run from a clean checkout on a suitable build machine:

```sh
nix run .#device-image-dist -- packages.aarch64-linux.rg353m-sd-image /tmp/rg353m-dist
nix run .#device-image-verify -- /tmp/rg353m-dist "$(git rev-parse HEAD)" --release
nix run .#device-image-dist-check
```

The destination must not already exist. The revision record uses the same
filename as the existing RetroArch distribution. Image names come directly from
the Nix SD builder; checksums use the standard `sha256sum` file format.
Compressed images are streamed through `zstd -19 -T2` during staging to reduce
release size without changing the decompressed disk image. This adds compression
time but does not write a second uncompressed image to the runner's disk.
Checksums describe the final, recompressed download.

## Runner and publication limits

The workflow uses standard GitHub-hosted runners and no external cache account,
paid runner, or self-hosted machine. ARM assembly uses two concurrent Nix jobs.
Odin's x86 kernel job uses one Nix job with four cores and removes unused Android,
.NET, and GHC SDKs from that ephemeral runner to make space. Both report disk use.
A successful image build does not establish hardware boot or gameplay behavior.

GitHub documents 14 GB storage for the standard ARM runner. If the build exhausts
storage, keep the failure visible and choose additional capacity or caching
based on that run. Do not silently fall back to building on a handheld.

GitHub Releases requires each asset to be smaller than 2 GiB. Oversized images
remain Actions artifacts; the release job fails with an explicit message.
It does not split files or publish a partial image.
