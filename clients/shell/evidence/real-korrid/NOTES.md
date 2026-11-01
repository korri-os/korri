# Shell against a real korrid (host mode), Xvfb 640x480, 2026-10-31

korrid: `nix build .#korrid` at merge 6c2dfc22a (`/nix/store/rvqvp6sz…-korrid-0.0.0`),
started by `proof/real-korrid.sh` (portal origin + private capability credential).
No games in its library, so both catalogs are `Empty`.

| Step | Result |
|---|---|
| declare / preferences / model / watch, Zone | loaded, first frame 126 ms, 61-62 fps; `zone.png` shows korrid's clockLabel and `Wired` |
| switch to Pico (F2) | loaded; one visible window afterwards; `pico.png` |
| RememberPreference through korrid | `app.surface.preferences` now returns `zone.look: blend`, `pico.font: tom-thumb` |
| declare with `displays` (zone "main", pico "all") | HTTP 422: korrid's `SurfaceDeclareRequest` is `deny_unknown_fields` and has no `displays` yet |

The `displays` run used copies of the packages with the `displays` line removed;
the committed packages declare it and need korrid's `displays` field.
