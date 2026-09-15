# Review examples

These are design illustrations, not complete builds. Proposed helper URLs
and functions are not verified published interfaces. No flake.lock files are
fabricated for unavailable inputs.

- `retroarch/cores.nix`: two catalogue entries produce separate plugins.
- `retroarch/cores/mgba/plugin.ts`: an ordinary module adds explicit mGBA
  settings and defaults. No private delta format.
- `retroarch/flake.nix`: the generation loop through a proposed shared helper.
  In the actual family repository, the helper is local code exported from
  that same flake. This example is a consumer of that helper for clarity.
- `frontend-override/flake.nix`: explicitly selects a frontend derivation.
  Another pinned derivation can replace it without replacing other plugins.
- `diy/`: full hand-written PPSSPP declaration and launch function, no family
  helper. Native escape-hatch handling is explicitly not implemented in this
  illustrative handler; it rejects rather than silently ignoring overrides.
- `settings.yaml`: device/person/game homes, using separate YAML documents.
- `manifest.example.json`: generated artifact shape for the DIY example.
  `<hash>` is a placeholder, not an actual store path.

The supplied TS types and examples pass a local TypeScript check. The Nix
files parse. No native packages were built or emulators executed.
