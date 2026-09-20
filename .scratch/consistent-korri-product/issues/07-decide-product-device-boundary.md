# Decide the shared product and hardware boundary

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 02, 04, 08, 09, 10

## Question

Which integration decisions belong to one shared Korri product, which belong to hardware support, and what checks prevent a new device or product change from silently omitting required behavior?

Use the agreed product, hardware support policy, bundled-plugin lifecycle, user-identity lifecycle, live-session behavior, and ROCKNIX-based emulator selections. Image sizing and variant/first-use readiness planning were explicitly removed from this effort; do not reopen them as hidden prerequisites. Inspect current ownership rather than assuming a new abstraction is needed. Load `/codebase-design` in addition to `/grilling` and `/domain-modeling` for this ticket.

Start with [shared device policy](../../../nix/base/default.nix), [SD format composition](../../../nix/formats/sd-card.nix), [Linux host composition](../../../services/inputd/nix/korri-linux-host.nix), [portal integration](../../../clients/portal/nix/nixos-module.nix), and the device modules that import them. Review the actual [image](../../../.github/workflows/device-images.yml) and [cache](../../../.github/workflows/nix-cache.yml) workflows too; shared source alone does not provide shared delivery.

Real cases include repeated runtime-account and package wiring, RP Mini V2 disabling shared streaming services, and Odin selecting a different kiosk path. Recheck each before using it as evidence. Separate legitimate hardware constraints from missing integration and obsolete product paths.

Resolve ownership, allowed variation, delivery obligations, and the observable acceptance contract with Simon. Include the cost and limits of the selected design. Record decisions and source pointers, not a speculative configuration schema. Newly exposed migration or recovery choices become child decision tickets or specific remaining fog. Finish this map only when no decisions remain before `/to-spec`.

## Comments

2026-09-20. Grilled with Simon in four rounds through the ask tool. Facts checked in source at `5a65ef4c` before each round.

Ownership today (`nix/devices/*/*.nix`): `nix/base` and `nix/formats/sd-card.nix` are imported by all six devices; `services/inputd/nix/korri-linux-host.nix` by five (not `rg35xxsp`); `clients/portal/nix/nixos-module.nix` by `r36tmax`, `rg353m`, `rgds`, `rpminiv2`; `services/kiosk/nixos-module.nix` by `odin2portal` only (`web-session.nix:59`); `services/korrid/plugin-host/nixos-module.nix` by `rg353m` and `rpminiv2` only; `nix/device-cache/nixos-module.nix` by four. `nix/base/default.nix` is a policy module (SSH off, autologin root, substituters, `stateVersion`); it composes no product service. No module assembles the product.

The three named cases: `users.users.gameplay` uid 1001 is repeated by hand in `r36tmax/portal.nix:14`, `rgds/portal.nix:45`, `rpminiv2/portal.nix:40`, while `odin2portal/runtime-user.nix:9` defines `korri`; `korri-linux-host` already takes `runtimeUser`, `runtimeUid`, `runtimeGroup`, `runtimeGid`. RP Mini V2 disables Sunshine by overriding `systemd.services.sunshine` and `korri-certificate-control` (`portal.nix:111-133`) and its `module-check.nix:184-189` asserts the disable; the comment says the shared host can stream but the first milestone cannot. Odin's kiosk crate (`services/kiosk`, Rust, CDP pipe intercepting `/runtime.json`) and the portal module (`korri-chromium-kiosk` with the private `window.KorriRpc` binding) were both last changed 2026-09-11; `AGENTS.md` names the binding path as the production portal.

Delivery: `.github/workflows/device-images.yml:42-48` accepts `rg353m|odin2portal|rgds|r36tmax` and refuses `r36tmax` until its loader notices ship; `nix-cache.yml:99` publishes the same four closures. `rpminiv2` and `rg35xxsp` have no delivery. `clients/portal/nix/deploy.py:67` copies with `--no-check-sigs`, which `nix/device-cache/README.md` forbids on devices. Checks: per-device `module-check.nix` for `r36tmax`, `rg35xxsp`, `rgds`, `rpminiv2`, plus `nix/base`, portal, kiosk, and device-cache checks, each hand-written and registered in `flake.nix:236-283`; `rg353m` and `odin2portal` have no device module check. `odin2portal/platform-policy.nix:158-161` disables sleep, suspend, hibernate, and hybrid-sleep. `nix/device-cache/README.md` states that plugin publication does not make Sunshine or korrid into plugins.

Simon changed the tree twice. Round 2: the streaming host must be disableable "like any other plugin", which makes it a plugin, not a product option. Round 3: device sleep will arrive in tiers, and every exposed cut-over becomes a child decision ticket, so this ticket does not close the map.

## Answer

Resolved 2026-09-20. All choices are Simon's, selected through the ask tool.

### Terms

- **Product module**: the one shared Nix module that composes every required Korri part. A device imports it and nothing else product-shaped.
- **Hardware fact**: a value only the device knows: kernel, display device and mode, rotation, renderer, input map, encoder, runtime limits. A device module supplies hardware facts.
- **Recorded limit**: an explicit statement, in the device module, that an optional hardware feature is absent or unverified on that device. It is the only allowed omission.
- **Product check**: the one evaluation-time check that asserts every required part is present in every exported device configuration.
- **Delivery**: a published SD image and its full closure in the signed cache, both from the same commit.

### Ownership

The product module owns every required part: base policy, Linux host, portal browser, plugin host, device cache policy, and the runtime account. A device module supplies hardware facts and recorded limits only. It may not add, remove, or swap a product service. Hand-disabling a systemd unit, as RP Mini V2 does today, is not an allowed variation.

The portal module with the private `window.KorriRpc` binding is the product browser path. Odin converges to it. [Ticket 12](12-decide-odin-browser-convergence.md#answer) subsequently selected retirement of the old kiosk launcher rather than an internal adapter, while preserving the separate Chromium transparency work.

The product owns the runtime account: one name and one uid on every device. [Ticket 11](11-decide-odin-runtime-account-cutover.md#answer) subsequently selected Odin's existing `korri` declaration and excluded account migration for alpha installations. That decision supersedes this ticket's original assumption that Odin needed a migration.

RG35XXSP is in the device set. Its console-only composition is a debugging state, not a hardware limit; it will import the product module like the rest.

### Streaming host

Hosting a stream is not a product part. It is a bundled plugin, on by default in most devices' plugin selection, removable through the plugin lifecycle from [Decide how bundled plugins remain removable](04-decide-bundled-plugin-lifecycle.md#answer). A device with no usable encoder omits it from its default selection instead of disabling units. This revises the `nix/device-cache/README.md` sentence that excludes Sunshine from plugins; [ticket 13](13-decide-streaming-host-plugin-boundary.md#answer) landed that rewording. Playing a stream stays a required behavior through available routes. Ticket 13 subsequently widened the plugin host's unit admission instead of moving the privileged parts behind korrid, bound each plugin's reach to its own approval, and put the streaming host in every current device's default selection except R36TMAX. It left korrid's Sunshine-specific certificate socket and seat wiring unresolved.

### Delivery obligation

A device cannot be a supported image without delivery: a published SD image and its complete closure in the signed cache from the same commit. Today that excludes `rpminiv2` and `rg35xxsp` (no workflow entry) and `r36tmax` (distribution hold). Developer tooling such as `clients/portal/nix/deploy.py` is outside the product; a supported image accepts only signed outputs, and that script is a development-only path.

### Checks and what they grant

One product check runs against every exported device configuration in the flake. A device that does not import the product module fails evaluation. Device-specific checks shrink to hardware facts.

A passing product check gates build and publication only. Supported status is granted by recorded physical acceptance, as [Define hardware limits and product acceptance](02-define-device-support.md#answer) rules. Simon also named the intended future gate: an automated on-device smoke run (boot, portal reachable, one launch) required for support, with physical tests only for changed behavior, once device-in-the-loop infrastructure exists. That gate is intent; no infrastructure supports it today.

### Physical acceptance set

First supported image per device model: fresh flash; normal setup with no manual fixes; browse and launch one local game per bundled runner; leave, return, and end per [Decide what happens when leaving and returning to a game](09-decide-live-session-behavior.md#answer); one streamed session if the device hosts or plays streams; sleep and wake if the device's sleep tier supports it; plugin install, remove, and rollback; identity backup export. Later releases: boot-and-play plus affected behavior. No numeric performance targets.

### Sleep

Device sleep arrives in tiers; some devices have no deep sleep yet, and later tiers may include ROCKNIX-style workarounds and hibernation. Whether a device with no sleep can be supported, and what the tiers are, is decided in a child ticket. This answer does not make sleep required or optional.

### Child decision tickets

Simon chose one child decision ticket per exposed cut-over, each blocked by this ticket:

- [Decide the Odin runtime account cut-over](11-decide-odin-runtime-account-cutover.md)
- [Decide the Odin browser path convergence](12-decide-odin-browser-convergence.md)
- [Decide the streaming host plugin boundary](13-decide-streaming-host-plugin-boundary.md)
- [Decide RG35XXSP product adoption](14-decide-rg35xxsp-product-adoption.md)
- [Decide device sleep tiers](15-decide-device-sleep-tiers.md)

The map is not complete until they resolve.

### Costs and limits

- One product module means a product change touches all six devices at once, and the refactor of six hand-assembled modules is real work before any device benefits.
- Retiring the kiosk crate removes Odin's only working browser path until the portal module runs there.
- Moving Sunshine into a plugin is a port, not a toggle: korrid must own the effects the plugin needs, and until the port lands no device has a supported streaming host.
- The product check proves parts are declared, not that they work. RP Mini V2's `transform 90` against its own check's `270` shows a check can pass while the config is wrong.
- Delivery as a support gate leaves three of six devices unsupportable until the workflows cover them.
- The physical acceptance list is a human judgement of "works"; two testers can disagree, and nothing measures performance.
- The `--no-check-sigs` rule is documentary; the script still exists and can be misused.

This answer authorizes no configuration schema, no plugin declaration format, no device write, and no deployment. Image sizing and variants stay out of scope.
