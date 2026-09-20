# Measure minimal and opinionated image sizes

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:task
Type: task
Status: open
Blocked by: 03, 04, 10

## Question

What measured size difference would the selected opinionated plugins add to a minimal image of the same product and hardware configuration?

This is an AFK measurement task that supplies evidence for the image-variant decision. It is not authorization to ship an image or implement missing product features.

Use the exact per-device emulator selections from [Validate per-device emulator defaults](10-validate-device-emulator-defaults.md), the approved remaining bundle policy, and the packaging decision. The policy alone is not an exact package list. Make a matched minimal-versus-opinionated comparison for each selected device model; do not extrapolate one model's size difference to the others. Within each comparison, keep the source revision, hardware configuration, compression, and product behavior the same, varying only the proposed bundled plugins. Report compressed download size and installed storage separately. Account for shared Nix dependencies rather than summing each plugin's full closure size. Identify the packages and configurations used so the comparison is reproducible.

Start with [image distribution tooling](../../../nix/formats/image-dist.py), [image build documentation](../../../nix/formats/IMAGE-BUILDS.md), and [device build policy](../../../nix/device-cache/README.md). Build only on suitable build machines. Do not flash a card, activate a device generation, publish a release, or bypass signature checks.

If required features or packages do not exist, measure the available subset and state the limit. Bounds or partial measurements are not a complete-image result. Surface any prerequisite that requires implementation as a separate decision; do not quietly build the missing product here. Record the measured facts as the answer. Simon chooses whether the difference is trivial in the dependent ticket.
