# korri-shell

The native shell for Korri surfaces (`docs/briefs/2026-10-31-native-surfaces-contract.md`).
It loads surface packages (`main.slint` + `logic.js`), runs their logic in
QuickJS, draws them with Slint's software renderer, and talks to the korrid on
its own device.

## Run

```
korri-shell --packages DIR [--cache DIR] [--surfaces ID,ID…] [--placement fullscreen|WxH] [--stats]
korri-shell install --packages DIR [--cache DIR]   # verify every package, write the index
korri-shell verify PACKAGE_DIR                     # one summary as JSON
```

Environment, as for `clients/linux`: `KORRID_ADDRESS` (127.0.0.1:PORT),
`KORRID_PORTAL_ORIGIN`, and `CREDENTIALS_DIRECTORY/KORRID_RPC_CAPABILITY`
(private to the user). The shell refuses to run as root and never writes the
capability or port to a file, URL or log. F2 or L1+R1 switches to the next surface.

Nix: `nix run .#shell-check` (fmt, clippy, tests), `nix run .#shell-run`,
`nix run .#shell-proof`, `nix build .#korri-shell`.

## Package

`globalThis.surface = { displays?, preferences, init, update, view, overlay? }`.
`displays` is `"main"` (the default when absent) or `"all"`; the shell passes
it in `app.surface.declare` and korrid publishes `model.displays` only for
`"all"`. Zone declares `"main"`, Pico `"all"`, as the portal registry does.

## Wire

`POST /rpc` with `app.surface.declare`, `.preferences`, `.model`, `.request`;
the model watch is a root WebSocket (`Origin`, then `Bearer CAP`, then
`app.surface.watch`), one push per ack. Treaty values go through the
`korri-surface-treaty` crate (`contracts/surface/`).

## Windows and placement

Only one shell window is mapped at a time: the Surface, or the Overlay during
the gameplay overlay. korrid places the one window owned by the processes of
the unit named in `KORRID_PORTAL_UNIT`, so the shell must run as that unit.
During a switch the new surface's window is mapped until its first frame, then
the old one is removed.

## Limits and performance

64 MiB QuickJS heap and 50 ms per call; a violation unloads the package and
restores the previous one. Slint is patched (`vendor/README.md`) to cache
still layers. Numbers and screenshots: `evidence/`.

## Backends

`winit` (desktop, default), `headless` (tests), `sdl` (webOS, through
`window-sdl/`, built with `--no-default-features --features webos`).
