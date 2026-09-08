---
date: 2026-09-08
topic: linux-plugin-installation
artifact: brief
---

# Linux plugin installation

## Chosen thing

A Linux device that has never encountered Tailscale can install, enable, update, and remove it without compilation or a NixOS system update. Tailscale remains optional.

The user accepted this requirement after reviewing the limits of the earlier generic host module proposal. That proposal supports independent updates for plugins already named in the system configuration. It does not meet the revised installation requirement.

## Users and context

Assume zero product users, as directed by the user. There is no requirement to preserve the proposed paths, options, or command interfaces. Existing development-device data still requires an explicit decision before removal.

The intended consumer is a device owner installing a redistributable plugin. ZIP delivery is an analogy, not a selected archive format.

## Goals

- Install a previously unknown plugin without adding its name or service definition to a NixOS generation first.
- Update and remove that plugin independently of the core Korri services.
- Receive native dependencies as prebuilt packages. Do not compile on the target device.
- Establish who authorizes privileged installation, using Tailscale as the first real case.

## Non-goals

Android integration, a plugin marketplace, a general permission vocabulary, backward compatibility for the rejected proposal, and device deployment are outside this design step.

## Constraints and grounding

The earlier Garnix choice is no longer viable. Its official shutdown notice scheduled hosted-service closure and artifact deletion for July 15, 2026: https://garnix.io/blog/shutting-down/. Replacement hosting remains unresolved. GitHub-based publication and plugin repositories are under discussion.

Concurrent commit `45694bb9` exports the unchanged pinned nixpkgs Tailscale package as `packages.<system>.korri-tailscale`. `plugins/tailscale/README.md` records its package boundary. Use that real producer rather than introducing another source pin.

`services/korrid/SCRIPTING.md` defines plugins as runtime-interpreted TypeScript or JavaScript declarations without I/O. Prebuilt native dependencies do not replace that declaration contract. No new declaration kind is approved here.

`services/inputd/nix/korri-bundle-module.nix` and `services/inputd/src/bundle_select.rs` provide existing selection and initialization behavior. They are references, not interfaces this design must preserve.

The system can include generic installation support beforehand. It must not require advance knowledge of Tailscale specifically. If the supplied kernel and modules lack a required capability, installation alone cannot provide it.

## Success criteria

Acceptance starts with a Linux device whose system configuration has no Tailscale-specific service or package registration. Generic host support and required kernel capabilities are allowed.

The device obtains prebuilt payloads, installs the plugin, and starts it through the selected authorization process. Tailscale authentication remains a separate required operation.

The test verifies a working connection, an independent plugin update, disablement, reboot behavior, and removal. It must distinguish process health from network availability. Failure testing must cover interrupted installation and an incompatible update. Returning to old binaries must not be described as application-data recovery.

Exact removal behavior for login state and recovery guarantees remain unresolved.

## Candidate shapes

| Shape | Benefit | Cost |
|---|---|---|
| Owner-approved privileged extension installation | Can install a new system service without teaching the generation its name beforehand. | The owner trusts the installed native payload with substantial device authority. This is not browser-style isolation. |
| Host-enforced restricted execution | Can limit each extension to operations the host permits. | Requires a permission and execution boundary grounded in Tailscale. Its feasibility for this case is not yet verified. |
| Advance per-plugin system registration | Closest to the previous module proposal. | Fails the accepted requirement and is not the selected direction. |

## Chosen shape and decisions

The user selected owner-approved installation of trusted system software. The installed native payload can receive administrator-level authority. Korri must not promise browser-style isolation. A package signature identifies a publisher, not safe behavior.

The installation behavior and trust model are selected. An isolated VM now demonstrates basic service installation through systemd portable services. This supports that mechanism as the next implementation candidate. It does not approve a final package contract or replace the remaining acceptance tests.

The previous proposed `services.korriPlugins` options, selector directory, units file, and CLI are not ratified by this brief. Paths, schemas, and policy formats must follow verified producers and consumers or an explicit user choice.

## Plugin repository sources

The app supports multiple plugin repositories. Here, a plugin repository supplies plugin listings and release references, not merely a plugin's source code.

The user selected catalogs served over HTTPS by any host. All repository sources follow the same catalog contract. GitHub is a possible host, not a required repository type. Device-side discovery and installation must not depend on GitHub-specific APIs.

- The app includes the official repository as a built-in source.
- The project owner alone curates the official repository and adds packages to it.
- Users can add their own repository sources without admission to the official repository.
- Adding a user repository does not add its packages to the official repository. Each repository retains its own publishing authority.
- No community-submission workflow is required for the official repository.

Repository registration does not replace owner approval for privileged software installation. The selected trust model above still applies.

The official catalog address, catalog schema, overlapping package identities, and source-trust delivery remain unresolved. The HTTPS decision does not select their formats or precedence rules.

## Open questions

Resolve package trust, explicit approval delivery, compatibility checks, update and recovery behavior, and state handling during removal. Test authenticated networking, firewall and DNS integration, Nix garbage collection, and the actual handheld kernels. Keep downloaded images reachable while services depend on them.

## Mechanism research

The locked nixpkgs revision `a6531044f6d0bef691ea18d4d4ce44d0daa6e816` already provides `pkgs.portableService` and a NixOS test for `systemd-portabled`. This is an evidence-backed candidate, not a new Korri package schema.

- `pkgs/build-support/portable-service/default.nix` builds a SquashFS `.raw` image containing service units and their Nix closures. Its arguments, naming rules, and image paths belong to nixpkgs and systemd.
- `nixos/tests/systemd-portabled.nix` tests runtime attachment, service start, and detachment. It does not test persistent enablement, reboot, or NixOS activation.
- `nixos/modules/system/boot/systemd.nix` includes `systemd-portabled.service` when the package supports it. The same module generates `/etc/systemd/system` from the NixOS generation.
- systemd documents persistent attachment under `/etc/systemd/system.attached/`, separate from that generated directory. See https://systemd.io/PORTABLE_SERVICES/.
- systemd v258.2 `src/portable/portablectl.c` uses `UNIT_FILE_PORTABLE` for enablement. It also discards some start and enable errors. An acceptance test must inspect actual unit state, not only the CLI exit code.
- The v258.2 `trusted` profile binds the host `/run` and does not set a restricted capability list. Tailscale still requires testing with the host network, device access, DNS, and its saved state.

Portable services can remove the need for custom unit-registration files and a custom service loader. The cost is an additional image containing its dependencies. This reduces Nix store deduplication across plugin images. Automatic application-health rollback remains outside portable attachment itself.

The portal already uses real Nix profiles for independent updates in `clients/portal/nix/select.sh`. Its current profile and health-check behavior are useful references. Do not assume a new custom selector implementation is necessary.

## Isolated VM evidence

On 2026-09-08, a throwaway x86_64-linux NixOS test passed with systemd 258.2 and Tailscale 1.90.9. It used the locked nixpkgs above, the packaged Tailscale unit, and `pkgs.portableService`.

| Test | Observed result |
|---|---|
| Install without advance registration | The generated host had no Tailscale service. `portablectl attach --profile=trusted --enable --now` installed it under `/etc/systemd/system.attached/`. The system generation did not change. |
| Start the actual daemon | `tailscaled.service` became active. The real CLI reported `BackendState=NeedsLogin`. The log showed creation of `tailscale0`. No account joined a tailnet. |
| Reboot | The service started again. A marker in its host state directory remained. |
| NixOS activation | Switching to a test specialization preserved the attached service and test state. |
| Detach | `portablectl detach --enable --now` stopped the service and removed its attached unit. Test state remained. Another reboot did not start the service. |

The VM used `max-jobs = 0` and an empty builder setting. Package and image construction happened on the development machine. The test did not fetch from Garnix, test an update, or demonstrate successful application-data rollback.

Two packaging requirements emerged from actual failures. The read-only image needs mount destinations for the vendor unit's `CacheDirectory=tailscale` and `StateDirectory=tailscale`. The packaged vendor unit also requires the `PORT` and `FLAGS` environment values normally supplied by the nixpkgs module. The probe extracted those defaults from that module and disabled upstream log upload. Neither requirement needed a Tailscale entry in the host generation.

Temporary evidence, not installed product code:

- Probe: `/tmp/korri-portable-host-probe.nix`.
- Command: `nix-build /tmp/korri-portable-host-probe.nix --no-out-link -j 2 --cores 2`.
- Successful result: `/nix/store/pcswp0i5v3ab8q1r5jw65l2dgpnd51lm-vm-test-run-korri-portable-host-probe`.
- Build log: `/tmp/pi-processes-faAeF6/proc_d3bb-stderr.log`.

Temporary files and unrooted store results can disappear. Preserve a maintained acceptance test in the implementation slice before claiming production support.

## Next implementation boundary

Prefer the standard portable-service mechanism over a new Korri unit loader unless the remaining tests contradict this result. The tested NixOS configuration already supplies the generic loader. Do not add a module that only renames that existing support.

The next slice must connect owner approval, prebuilt-package acquisition, durable image retention, and actual service-state checks. Ground its inputs in a real Tailscale package and the existing Nix/systemd interfaces. Do not revive the earlier `units` file or custom selector paths by default.

## Discussion references

The prior proposal and its source references are recorded in `.pi-web/handoffs/8e35c755-f070-484f-9afd-558a866250ba.md`. Later user discussion supersedes its generic-module implementation direction. Parallel work must recheck this brief before freezing that proposed contract.
