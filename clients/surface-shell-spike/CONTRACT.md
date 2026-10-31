# THROWAWAY SPIKE: surface packages loaded by a Rust shell at runtime

This contract binds three parallel pieces of one spike: the shell, the Zone
package and the Pico package. It is spike-only. It is not Korri's surface
treaty and it defines no persisted schema.

## The question

Can a fixed native Rust shell load a surface as plain files (`.slint` views and
TypeScript logic) at runtime, and switch to another surface while running, on
the RG35XX SP, so that it feels good?

## Layout

```
clients/surface-shell-spike/
  CONTRACT.md            this file
  fixtures/model.json    one SurfaceModel (contracts/surface/korri-surface.ts),
                         generated from Zone's fixtureReadyModel by gen-model.ts
  Cargo.toml, src/, package.nix, device/     the shell (shell worker only)
  surfaces/zone/         the Zone package (Zone worker only)
  surfaces/pico/         the Pico package (Pico worker only)
```

## A surface package

A folder with exactly these entry points, and anything else it needs beside
them (fonts, art, more `.slint` files):

- `main.slint` exports `component Surface inherits Window`. It declares an
  `in property` for every value it shows. It declares no callbacks and handles
  no keys: the shell delivers all input. It imports only files inside its own
  folder (no `std-widgets.slint`). It must lay out at 640x480 and at 1280x720
  from the window size alone. Animations live here.
- `logic.ts`, bundled to `logic.js` with
  `bun build logic.ts --format=iife --target=browser --outfile=logic.js`.
  Both files are committed. Running it defines:

  ```ts
  globalThis.surface = {
    init(model: SurfaceModel): State,
    update(state: State, message: Message, model: SurfaceModel): State,
    view(state: State, model: SurfaceModel): View,
  }
  ```

  All three are pure: no timers, no `Date`, no `Math.random`, no I/O, no
  globals other than `surface`. A package may import its own surface's real
  code from `surfaces/<id>/src/state/` (types from the treaty are erased).

## Messages the shell sends

```ts
type Message =
  | { _tag: "Pressed"; action: "up" | "down" | "left" | "right" | "confirm" | "back" | "menu" | "options" }
```

A held direction repeats: first repeat after 400 ms, then every 100 ms (the
portal's own timings, `clients/portal/src/input/native-gamepad-mapper.ts`).

## The view

`view()` returns one plain JSON object. The shell turns it into the Surface's
properties:

| View JSON | Slint value |
| --- | --- |
| top-level key `fooBar` | property `foo-bar` (camelCase becomes kebab-case) |
| object `{ "$image": "<data: URL or file path>" }` | `image` (the shell decodes and caches it) |
| any other object | struct; its keys follow the same camelCase rule |
| array | model of the converted items |
| string, number, boolean | string, float or int (by the declared type), bool |
| `null` or a missing key | the property keeps its value |

Colours, sizes and every other styling decision stay in `.slint`.

## The shell

- Opens one full-screen window (`SHELL_FULLSCREEN=1`) or a 640x480 window
  (`SHELL_SIZE=WxH` overrides). Software renderer, winit Wayland and X11.
- Loads `fixtures/model.json` once and hands it to every surface.
- Loads packages from `surfaces/` in the order `zone`, `pico` (overridable
  with `SHELL_SURFACES=a,b`), and shows the first.
- **Switch:** L1 and R1 held together on the gamepad, or F2 on a keyboard,
  loads the next package: compile `main.slint` with `slint-interpreter`, start a
  fresh QuickJS context (rquickjs 0.14, korrid's engine), evaluate `logic.js`,
  `init`, `view`, show it, drop the previous one. Surface state starts fresh.
- **Input:** keyboard arrows, Return = confirm, Escape or Backspace = back,
  M = menu, O = options; gamepad via evdev with the portal's codes
  (0x130 confirm, 0x131 and 0x13a back, 0x134 options, 0x13b menu, the hat or
  the left stick for directions). Each message goes through `update`, then
  `view`, then the properties.
- With `SHELL_STATS=1` it logs, to stderr, each switch's compile, logic-load
  and first-frame times, and frames a second every 5 s.
