---
title: Add HTTPS plugin repositories
type: feat
status: active
date: 2026-09-08
origin: docs/briefs/2026-09-08-linux-plugin-installation-brief.md
---

# Add HTTPS plugin repositories

## Summary

Extend the implemented administrator plugin host with HTTPS catalogs and explicit repository selection. Publish prebuilt downloads that can live on GitHub Releases or another HTTPS host. Keep the current service policy, approval flow, and lifecycle recovery instead of introducing another installer.

The first delivery uses the CLI. It does not add an in-app management screen.

## Requirements

| ID | Requirement and grounding |
|---|---|
| R1 | Include one owner-curated official repository and support user-added repositories. The owner alone controls the official package list. |
| R2 | Accept catalogs from any HTTPS host. Device behavior must not depend on GitHub APIs. |
| R3 | Adding the URL trusts that host. No separate catalog-signing key is required. Validate downloaded bytes against the trusted catalog. |
| R4 | Keep duplicate plugin listings separate by source. The user selects a repository explicitly. Updates remain tied to it until an explicit switch. |
| R5 | Download prebuilt software only. Missing or corrupt data must not trigger local or remote builds. Preserve Nix integrity verification. |
| R6 | Reuse the implemented administrator approval and service lifecycle. Install starts disabled; updates preserve desired state; removal preserves data unless purge is explicit. |
| R7 | Publish complete immutable releases without a paid cache service. Keep large downloads out of Git history and Pages. |
| R8 | Prove Tailscale installation and same-source update from HTTPS, plus a second source with overlapping identity. Preserve the existing cold-host lifecycle tests. |

R1–R4 come from the user's repository decisions in the origin brief. R5–R6 use `services/korrid/plugin-host/README.md`, `src/package.rs`, and `src/host.rs`. R7–R8 apply the confirmed CLI-first implementation scope.

## Existing contracts to preserve

- `services/korrid/plugin-host/src/declaration.rs` owns plugin identity and the daemon declaration. `plugins/tailscale/plugin.ts` is a real producer.
- `plugins/tailscale/package.nix` produces the immutable declaration package. Nix supplies package version, platform, and store output identity.
- `services/korrid/plugin-host/src/package.rs` imports exact store outputs, verifies the closure, and computes the effective policy report. A catalog must not supply an approval or rendered unit policy.
- `services/korrid/plugin-host/src/host.rs` owns receipts, desired state, host locking, recovery, and GC roots. Repository origin must follow the same transaction as package selection.
- `services/korrid/plugin-host/src/storage.rs` owns private, durable writes. Source state must not collide with the receipt-directory enumeration in restore.
- `services/korrid/plugin-host/src/main.rs` supplies the existing administrator CLI. Keep raw-cache operations distinct from catalog operations. Never guess whether a URL is a catalog or a binary cache.

Do not redesign the declaration, add a permission vocabulary, or switch the service adapter back to portable images.

## Delivery decision and proof gate

GitHub Releases hosts files, not a standard Nix cache directory. Publish a complete standard Nix file-cache export inside an archive. The device verifies the archive hash, stages it safely, and passes its local cache to the existing Nix importer.

First prove Nix's existing content-addressed conversion on the build machine. Content-addressed paths derive their identity from their bytes. They do not need an additional publisher key to satisfy Nix's integrity model. This matches the selected HTTPS-origin trust without weakening `require-sigs` for ordinary input-addressed paths.

Grounding: Nix 2.31 documents `nix store make-content-addressed`, recursive conversion, and signature-free verification of content-addressed outputs at https://nix.dev/manual/nix/2.31/command-ref/new-cli/nix3-store-make-content-addressed. The existing importer already runs recursive Nix verification.

This interface is experimental. U1 must prove compatibility with the locked Nix and the actual Tailscale binaries before the rest of the delivery path depends on it. If the proof fails, stop for a transport decision. Do not substitute `--no-check-sigs`, automatically trust arbitrary signing keys, or build on the device.

The cost is conversion work on the builder and larger self-contained downloads. Rewritten dependencies do not reuse the original input-addressed store paths. The complete archive must fit GitHub's per-asset limit; do not promise an unmeasured size.

## Catalog contract discipline

The catalog producer and reader must share one strict serialization contract. Derive its data from the real declaration, Nix output metadata, and the archive producer's actual URL and digest. Do not publish device-specific approval or unit data as repository authority.

Create the first valid record from the actual Tailscale package. Document the grounding for each field beside that producer. Do not invent channels, dependency resolution, pagination, compatibility versions, or future payload kinds. Release selection is explicit; no universal version ordering is needed.

The catalog cannot mark itself official. Official status comes from the host's configured source. Repeated identities from different repositories are alternatives, not conflicting installed copies. Ambiguous duplicate release/platform records inside one catalog must fail.

## Implementation units

### U1. Produce and verify a redistributable package

**Requirements:** R3, R5, R7, R8. **Dependencies:** None.

**Files:** Extend `services/korrid/plugin-host/src/lib.rs` and `Cargo.toml`; introduce focused catalog and publisher modules under `services/korrid/plugin-host/src/`, with tests in `services/korrid/plugin-host/tests/catalog.rs` and `tests/publish.rs`. Extend `services/korrid/plugin-host/package.nix` and `default.nix` for the build-side command. Use `plugins/tailscale/package.nix` unchanged unless a real producer requirement demands a change.

**Approach:** Prove content-addressed conversion and actual executable behavior first. Export the complete converted closure using Nix's file-cache format. Produce one bounded archive and a catalog record through the same Rust contract the reader consumes. Keep the publisher usable without administrator privileges on the build machine. Distinguish plugin release identity from the upstream Tailscale binary version.

**Execution note:** Test the public producer and importer with actual store paths and empty destination stores.

**Test scenarios:** Round-trip the real declaration; reject unknown fields and malformed identities; preserve separate platform variants; reject ambiguous repeated records; execute both converted Tailscale binaries; import a complete archive without publisher keys while Nix integrity checks remain active; reject tampered cache content and incomplete closures; reject unsafe archive paths, links, and oversized data.

**Verification:** A fresh store accepts the converted package through the existing importer, including its recursive `--sigs-needed 1` verification, without compilation or signature bypass. Its declaration and executable versions match the producer. Failure of that exact import/verify combination stops U1. Archive extraction writes only inside staging.

### U2. Add HTTPS source management and catalog inspection

**Requirements:** R1–R4. **Dependencies:** U1 contract.

**Files:** Add repository acquisition and source-state modules under `services/korrid/plugin-host/src/`; extend `src/main.rs`, `src/lib.rs`, `src/storage.rs`, `package.nix`, and `nixos-module.nix`. Add `services/korrid/plugin-host/tests/repository.rs`.

**Approach:** Use a real HTTPS client with certificate validation, bounded redirects, timeouts, and response sizes. Persist source URLs through the private storage boundary. Configure the official source through composition; do not invent a production address. List sources and releases without importing packages or evaluating downloaded plugin source. Keep origins visible. Removing a repository removes discovery/update access, not installed software or its private state.

**Patterns:** Existing bounded helpers in `src/process.rs`, durable writes in `src/storage.rs`, and the root-owned host configuration. One storage owner defines the source-state and staging locations and updates restore's entry classification. Recognized source/staging entries must not become receipt errors; unrelated unexpected entries must still fail.

**Test scenarios:** A real local HTTPS server with a trusted test CA; invalid certificate and HTTPS downgrade refusal; malformed/oversized catalog; timeout and partial response; repeated source addition; two sources offering the same plugin; forged official status; source removal without a service mutation; clear missing-official-source configuration before publication.

**Verification:** The CLI adds, lists, inspects, and removes sources deterministically. No GitHub-specific request or payload execution occurs during discovery. Tests use a real server, not network interception.

### U3. Bind installation and updates to the selected source

**Requirements:** R3–R6, R8. **Dependencies:** U1, U2.

**Files:** Extend `services/korrid/plugin-host/src/main.rs`, `src/host.rs`, `src/package.rs`, and the repository modules. Extend `tests/host_boundary.rs` and `vm-test.nix`.

**Approach:** Resolve an explicit source, plugin identity, and release before downloading. Validate the archive hash before import. Use a controlled local cache URI, never untrusted Nix store options from a catalog. Re-evaluate the imported declaration and bind approval to the exact source, package, and effective policy. Commit origin with package selection, so recovery cannot retain a new package with an old source. Preserve the real direct-cache administrative path as a distinct input route, not a catalog fallback. A source switch always requires fresh inspection and approval, even when the package bytes coincide. Approval binds to the stable repository selection, never a temporary staging path.

Local status, enable, disable, remove, and recovery use the committed receipt and store. They require neither a reachable catalog nor continued membership in the configured source list. Source registration is required for new repository downloads. Removing a source does not destroy installed provenance or prevent local cleanup.

A single staging owner creates private, no-follow staging directories and enforces compressed and expanded size bounds. It cleans up successful and failed operations. Recovery removes its own stale staging after acquiring the host lock. Downloaded bytes cannot change between hash validation and import. Do not delete arbitrary temporary paths supplied by metadata.

Receipt changes are a clean cut with explicit provenance for both real input routes. Do not add fallback reads or runtime migrations. An old receipt without that provenance fails clearly and remains untouched. Any existing development receipts need an explicit operational decision before deployment; no source may be guessed for them. Preserve restore's existing error collection and the ability to recover unaffected plugins.

**Test scenarios:** Selected-source install; mismatched declared identity/platform/hash; changed catalog between inspection and installation; same-ID alternative from another source; unavailable selected source despite an available alternative; explicit source switch requiring approval; disabled update remains disabled; failed update restores package and source together; removing a source leaves installed state intact; interrupted staging/import/selection preserves recovery; another plugin's PID stays unchanged.

**Verification:** The maintained cold-host VM installs and updates real Tailscale through HTTPS catalog selection, exercises a second repository, and preserves the existing authenticated-network and power-loss gates. The system generation does not change.

### U4. Publish from GitHub and remove obsolete Garnix wiring

**Requirements:** R1, R5, R7, R8. **Dependencies:** U1–U3.

**Files:** Add `.github/workflows/plugin-repository.yml`; remove `garnix.yaml` after retaining the relevant plugin and host checks in the new workflow. Update `flake.nix`, `nix/device-cache/nixos-module.nix`, `module-check.nix`, and `README.md`. Update `plugins/tailscale/README.md`, `services/korrid/plugin-host/README.md`, and `docs/research/runtime-plugin-host.md`. Limit `nix/tasks.nix` changes to the recorded ambient-nixpkgs CI failures and publisher wiring.

**Approach:** Build on x86_64 and ARM64 workers, validate outputs, then publish complete immutable Release assets. Publish the small catalog separately on Pages or another HTTPS host. Keep publication owner-triggered and protect write credentials from untrusted contributions. Build only the declared plugin publication targets and host checks. Start with the existing Tailscale export on both Linux architectures. Core packages without a plugin declaration are not registry entries. Remove Garnix URLs and keys; keep the download-only policy and default NixOS cache. Replacing the old general-purpose core binary cache is separate from plugin release hosting.

**Test scenarios:** Both architecture outputs; missing architecture or failed checks prevent publication; no publishing credentials in PR validation; catalog references only completed assets; archive size stays below GitHub's limit; clean-runner commands do not require an ambient NIX_PATH; publisher output works from a non-GitHub HTTPS server.

**Verification:** Local publication artifacts pass the same reader and VM tests. Live publication is complete only after the owner supplies the official destination, approves external changes, and the actual GitHub build/download is verified.

## Scope boundaries

No app management screen, Android installer, private-repository credential flow, automatic updates, repository priority, general dependency resolver, new permission vocabulary, or application-data snapshot rollback. Preserve current disable/remove/purge behavior.

No physical-device access, firmware writes, real tailnet enrollment, paid service, repository creation, release publication, or push is authorized merely by this plan. Those need their operational approval.

## Open execution and publication items

- U1 must verify the experimental content-addressed conversion with the locked Nix. This is not yet runtime-verified for Korri.
- The precise catalog serialization comes from the paired producer and consumer, using the listed real inputs. Record it before exposing independent publishers.
- Supply the real official catalog URL before shipping the default source. No illustrative URL may become a production default.
- Test archive size and limits with actual outputs. GitHub permits assets under 2 GiB; Pages is only for the small catalog.
- The two existing GitHub workflows failed on missing ambient nixpkgs lookup. Their recorded follow-up is `01M214N8E7SMPSS36QZ02ZMKCD`. Do not broaden that fix into unrelated application work.

## Completion

Run Rust formatting, Clippy, unit and public CLI tests, the maintained HTTPS/Nix/systemd VM acceptance, and relevant Nix module checks. Evaluate both architecture outputs. Review the source-trust, import, archive, and recovery boundaries before landing. Report local code completion separately from live publication and physical-device acceptance.
