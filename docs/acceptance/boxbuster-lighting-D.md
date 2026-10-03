# Boxbuster lighting D

The user chose D from the renderer screenshots: warm shelf lights and blue TV
spill. The implementation keeps the room dark. It adds no overlay, brightness
setting, persisted data, or device surface change.

## Grounding

The comparison assets are in the workspace at
`.forgerie/attachments/boxbuster-lighting/`. The selected references are
`shelf-mixed.png` and `tv-mixed.png`. They are real renderer captures with a
fixed camera and a demo library, not image edits. The user approved their
lighting, including the warm tubes on both shelf faces.

`map.ts` supplies gondola positions, shelf levels and `TV_SCREEN`. The chosen
study supplies the tube dimensions, colour and light falloff. `store-lighting.ts`
converts those inputs into renderer uniforms and tube transforms. This adds no
configuration schema.

## Implementation

The existing PS1 material receives local light before its dither and fog steps.
Ambient light, camera, colour quantization and resolution stay unchanged.
Emissive screens, focused tapes, signs and floor marks retain their own colours.
The tubes use one instanced draw call, up to 96 tubes for the largest current
map. Each store owns its lights; no browser-global state or preview switch ships.

The TV spills blue light when it shows a loaded tape, work or a problem.
Standby produces no spill. The selected blue stays fixed rather than sampling
artwork. The shader uses local attenuation, not shadow maps. It does not model
occlusion through walls. GPU cost on a handheld remains unmeasured.

## Verification

| Check | Result |
|---|---|
| Surface tests and typecheck | 120 tests passed; typecheck passed. |
| Portal tests and typecheck | 365 tests passed; typecheck passed. |
| Portal bundle and immutable Nix package | Both built successfully. |
| Selected D screenshots, shelf and TV at 640×480 | Pixel-identical in Chromium with software WebGL. |
| Browser captures at 640×480, 480×640 and 1920×1080 | Nine captures; no browser or shader errors; no DOM text over the room. |
| TV transitions on one mounted scene | Off → picture → working → problem → off matched fresh captures in all four states. |
| New renderer modules, Biome lint | Passed. |
| Existing scene and tape lint findings | Existing array-index key and effect-dependency findings remain unchanged. |

Screenshots, comparison metrics and build logs are saved under
`.forgerie/attachments/boxbuster-lighting-D/` in the workspace. Preview files
were removed before the production package was built.

## Device delivery

The read-only selector probes timed out on `root@192.168.1.239:22` and
`sobo:2222`. No device was changed. The bundle contains the lighting, but no
physical-device rendering or performance claim is made.

The existing `portal-deploy` helper uses `--no-check-sigs`, which conflicts with
`nix/device-cache/README.md`. This slice does not change deployment tooling.
Instead, the workspace retains a scoped deployment script that signs this exact
verified bundle with the existing trusted builder key, uses normal `nix copy`
signature checks, then calls the installed portal selector. It builds nothing
on the target and preserves the device's surface selection.

When the device is reachable, from the development machine:

```sh
nix shell nixpkgs#python3 nixpkgs#nix nixpkgs#openssh --command python3 \
  .forgerie/attachments/boxbuster-lighting-D/deploy.py root@192.168.1.239
```

The script checks connectivity before it copies or restarts anything. It needs
the trusted builder signing key described in `nix/cache/README.md`. Selector
health checks verify bundle delivery, not that Boxbuster is the selected surface.
