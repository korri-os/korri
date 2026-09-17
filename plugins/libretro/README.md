# The libretro core catalogue

One catalogue produces one plugin package per core. Installing mGBA does not
install every core, and removing it does not disturb the others.

| File | What it holds |
|---|---|
| `cores.nix` | The catalogue. One entry per core |
| `core-plugin.nix` | Turns one entry into one plugin package |
| `retroarch.ts` | The shared launch helper every generated core uses |
| `example-check.nix` | Proves korrid's committed example still matches the generator |
| `frontend-override-check.nix` | Proves one core can name its own frontend build |
| `default.nix` | Maps the catalogue to `korri-plugin-<name>` packages |

## Adding a core

Add an entry to `cores.nix`:

```nix
gambatte = {
  title = "Gambatte";
  description = "Runs Game Boy and Game Boy Color content with the Gambatte libretro core.";
  core = pkgs.libretro.gambatte;
  coreFile = "${pkgs.libretro.gambatte}/lib/retroarch/cores/gambatte_libretro.so";
  systems = {
    gb = { title = "Game Boy"; extensions = [ "gb" ]; };
    gbc = { title = "Game Boy Color"; extensions = [ "gbc" ]; };
  };
};
```

That is the whole change. The package `korri-plugin-gambatte` appears, with the
plugin id `@korri:gambatte`, the runner `@korri:gambatte/gambatte`, its own
session controls, and one discovery claim per system.

Name the core file exactly. A file name is not derived from a package name,
because the two disagree: `beetle-pce-fast` ships
`mednafen_pce_fast_libretro.so`.

Keep extensions conservative. An extension owned by several systems, such as
`bin`, stays out until a real library needs it.

## Naming a different frontend

An entry may add `frontend` to choose the RetroArch build that core runs:

```nix
mgba-nightly = {
  title = "mGBA (nightly frontend)";
  description = "Runs Game Boy Advance content on a pinned RetroArch build.";
  core = pkgs.libretro.mgba;
  coreFile = "${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so";
  frontend = pinnedRetroarch;
  systems.gba = { title = "Game Boy Advance"; extensions = [ "gba" ]; };
};
```

Omit it and the core takes nixpkgs' RetroArch, which is what every entry does
today. The override selects the **build**, never the protections: Korri's
read-only patch and its compiled source regression still run against whatever
is chosen, and a fork the patch cannot apply to fails the build rather than
quietly losing the protection.

The settings evidence follows the selected binary. A core answering
`settings.describe` from the default build's schema while running a different
build would report keys its program does not have, so the schema is re-derived
from the exact program.

Overriding one core changes that core only. `frontend-override-check.nix`
builds three cores and reads their manifests to hold all of it: the program and
the settings evidence move, the libretro core does not, and a second core keeps
the default frontend.

## What a generated plugin is, and is not

A generated plugin is an ordinary plugin. It is built by the same builder,
admitted by the same rules, and has no interface a hand-written plugin lacks.

Each core carries **its own RetroArch** inside its own closure. Two cores
installed together share one store path, so the disk cost is paid once, but
neither can break the other by being removed. `@korri:retroarch` names a
settings family, not a package a core depends on.

Two cores may claim one system. Genesis Plus GX and PicoDrive both claim Mega
Drive. Discovery returns both claims and the player chooses the runner.

## The runner owns its settings

`settings.ts` is generated beside the plugin from the pinned RetroArch source:
the keys that build actually reads, and their types. The runner answers
`settings.describe` and `settings.validate` from it, and leaves out of the
native configuration anything this build cannot apply. korrid holds no key
table and reads no settings file; it reports what the runner says as launch
warnings and never rewrites what a person authored.

## Adding behaviour to one core

A per-core source file may sit beside the catalogue and export handlers the
generated file does not supply. Nothing in the contract is reserved for
generation: whatever a hand-written plugin may export, a generated one may too.
