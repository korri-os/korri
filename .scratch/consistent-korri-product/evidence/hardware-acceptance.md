# Hardware limits and acceptance evidence

Decision: [Define hardware limits and product acceptance](../issues/02-define-device-support.md)

## Review boundary

This source review used `f7b6eb66`. Two read-only scouts inspected device composition and acceptance checks. The parent checked the main source examples below. No builds, tests, device connections, or physical observations were performed. Historical reports identify what was tested then, not the state of a currently installed image.

## What the examples actually establish

| Case | Source evidence | Limit of the evidence |
|---|---|---|
| RP Mini V2 intentionally omits services. | [Product composition](../../../nix/devices/rpminiv2/portal.nix), lines 89-105, 121-133, and 157-164, disables audio, remote input, Sunshine activation, and physical networking. [Module checks](../../../nix/devices/rpminiv2/module-check.nix), lines 184-195, assert several of those omissions. | Passing the corresponding assertions would prove the intended limited configuration, not the full agreed product. Disabled software is not proof of absent physical capability. |
| RG35XXSP has a console-only composition. | [Device composition](../../../nix/devices/rg35xxsp/default.nix) imports only the [SD image](../../../nix/devices/rg35xxsp/sd-image.nix), which selects diagnostic root-console access without the portal. The [hardware summary](../../../nix/devices/rg35xxsp/README.md) lists a display and wireless hardware. | This is a bring-up configuration, not evidence that the device is physically screenless or unable to run the product. |
| RG DS audio remains unverified. | [Portal composition](../../../nix/devices/rgds/portal.nix), lines 79-82, disables audio and explains that routing is unverified. | This is an integration/verification gap, not evidence of absent audio hardware. |
| R36T Max codec limits are driver-specific. | [Codec report](../../../nix/devices/r36tmax/CODEC-PATH.md), lines 90-120, reports a mainline driver with H.264 decoding but JPEG-only encoding. Only the H.264 fixture passed; an earlier JPEG test faulted. | The report does not establish that the silicon lacks H.264 encoding. Vendor-driver work remains separate, and the short decode fixture is not a sustained streaming benchmark. |
| Odin encoder probes differ from the shipped service. | [Encoder acceptance report](../../../docs/acceptance/sunshine-korri-v4l2m2m-portal-2026-09-05.md), lines 3-11, records codec/resolution tests with a temporary Iris module and root-run Sunshine. [Current session configuration](../../../nix/devices/odin2portal/web-session.nix) selects software encoding. | Hardware capability was demonstrated under a bounded test setup. The report explicitly leaves service integration, startup loss, and moving-content throughput unresolved. |
| RG353M audio has bounded historical evidence. | [Persistent audio report](../../../docs/acceptance/rg353m-persistent-audio-2026-09-06.md), lines 25-28, records post-reboot encoding, first audio packets, and sink-monitor samples. | The report explicitly excludes physical audibility and decoded-client fidelity. It documents a reboot, not a power-removal cold start. |
| An unselected plugin is a separate case. | [RP Mini V2 plugin integration](../../../nix/devices/rpminiv2/game-plugins.nix) describes receipt-driven selections and generic host support. | This review did not inspect an installed device's receipts. Do not infer that a specific runner is absent, or equate a user's plugin choice with a missing product integration. |

None of these examples proves a physically absent capability. The review distinguishes source omissions, driver limits, and test boundaries without assigning hardware incapability by inference.

## Existing delivery checks are narrower than product acceptance

[Shared module checks](../../../nix/base/module-check.nix) evaluate configuration policy. Device module checks verify selected boot, service, input-package, and image properties. The [image workflow](../../../.github/workflows/device-images.yml) builds and stages artifacts, then explicitly labels publication as an unverified prerelease. The [cache workflow](../../../.github/workflows/nix-cache.yml) publishes prebuilt software; publication is not runtime acceptance.

The [R36T Max update restriction](../../../nix/devices/r36tmax/sd-image.nix) refuses normal boot-generation installation because its FAT update path is not verified. Its [README](../../../nix/devices/r36tmax/README.md), lines 56-65, requires a full SD-image rewrite for boot updates and records a separate distribution hold. Downloading a closure is not proof that the next boot selects it.

Existing [device-cache policy](../../../nix/device-cache/README.md) requires verified prebuilt delivery and preserving working software after a cache failure. Existing repository rules require explicit owner readiness for physical tests and separate approval for device writes. This planning review grants neither.

## Additional source drift found

Medium importance: RP Mini V2 [product composition](../../../nix/devices/rpminiv2/portal.nix) selects `transform 90`, while its [module check](../../../nix/devices/rpminiv2/module-check.nix), lines 181-182, requires `transform 270`. The [README](../../../nix/devices/rpminiv2/README.md) also still describes 270 degrees. These are verified source disagreements; this review did not run the Nix check or retest orientation. No repair was attempted.

This is another reason to distinguish an old acceptance record, a current source assertion, and a test of the actual release image. The policy decisions belong in the parent ticket, not in this evidence document.
