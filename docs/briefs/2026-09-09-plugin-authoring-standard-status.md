# Plugin standard implementation status

This status accompanies `2026-09-09-plugin-authoring-standard-brief.md`.
The brief records decisions, not completion. Local integration of the completed
slices does not mean the end-to-end rollout is complete.

## Implemented slices

- Named ES-module exports, bounded callback execution, generated publisher
  identity and full-key signature verification for cold and cached packages.
- Nix-generated native service units, directive validation, host hardening,
  declared IPv4/IPv6 ports and lifecycle cleanup.
- Approved installed game declarations, exact dependency references,
  dependency-first recovery and isolation from unrelated corrupt receipts.
- Real RetroArch and mGBA plugin packages, installed Linux registry, runtime-user
  callback execution and per-runtime savestate separation. Android retains its
  separate existing platform declarations.
- Current/previous approved selections, dependency-safe rollback, offline
  `restore ID` and boot `restore-all` reconciliation.
- Backend route candidates, saved system/game runtime choices, per-launcher
  configuration cascade, explicit selected launch and visible setting warnings.
  Runtime preference writes have durable signed-request replay protection.

The six original core commits were rebased without content changes. The routes
commit was reapplied after a test-fixture overlap; exact dependency pins and all
route assertions were retained. Independent review findings were fixed before
integration.

## Verification

- Combined korrid Rust tests passed after integration.
- Portal checks passed after integration.
- Rust formatting passed.
- `nix run .#korri-plugin-check` passed, including the combined plugin-host VM.
  Logs: `/tmp/plugin-integration-korrid.log`,
  `/tmp/plugin-integration-portal.log`, `/tmp/plugin-integration-host-vm.log`.

Focused tests were used during the later work. One combined VM run gates this
integration, rather than another VM run per commit.

## Still unfinished

- Portal runtime chooser wiring. The backend methods are documented in
  `services/korrid/ROUTES.md`; the surface does not yet use them.
- RetroArch source-backed typed-settings metadata and its renderer. Until that
  producer lands, requested typed settings generate omission warnings. The
  current code must not be described as complete typed-policy support.
- Production publisher core-lock cutover, publication and core CLI commit lookup.
  The separate `korri-plugins` branch `feat/plugin-standard` contains local commit
  `ad103dcc`, but its core lock still points to the previous published API. Do
  not merge or publish it against that lock. Do not commit a local-path lock.
- Haku generation integration, explicit receipt/data cutover, deployment,
  Tailscale account login/connectivity and physical gameplay verification.
- SSH plugin implementation and an explicit privilege/authentication decision.
  Existing recovery SSH must remain available while that plugin is proved.

No device deployment or GitHub push is implied by this local integration.
