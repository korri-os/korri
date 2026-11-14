# Stage 1 / slice 3 — `@korri:viewer` and korrid's selection (#435)

Korri: `0967e169b`..`fa70de684`, with this record on top, on `origin/main`
`056dec9d3`. Plugins: `feat/viewer-plugin-435` at `1fb0c6d`, with the Korri
input pinned at `fa70de684`.
Everything was built on zao. Nothing was installed on a device, published,
pushed or landed.

## Cold NixOS guest: `korri-viewer-lifecycle`

`vm-lifecycle.txt` is the test driver's own output, filtered, from
`/nix/store/fanibcqc9cyd3ddak65cjnqxsan493hp-vm-test-run-korri-viewer-lifecycle`.
The same build passed `korri-viewer-plugin-native`, `korri-viewer-picture`,
`korri-viewer-io` and `korri-moonlight-plugin-native`. The check is
`services/korrid/viewer-vm-test.nix`, run by the plugins repository with the
real `@korri:viewer`, the same plugin with one byte of its treaty changed,
and `@korri:moonlight`.

| Step | Result |
| --- | --- |
| The plugin host restores both seeded viewers; both setup units run | pass |
| korrid's selection (`installed_viewer`, as user `korrid` in korrid's sandbox) starts `@korri:viewer` against a private Sunshine in the guest | pass |
| Unit command: `--lifecycle-socket /run/korri-game-<launch>/control/viewer.sock --launch-id <launch> --controller-uid 976`, `-viewonly` | pass |
| Runtime directory: owner 1000, `user::rwx`, `user:976:--x`, `group::---`, `mask::--x`, `other::---` | pass |
| Socket: `user:976:rw-`, `other::---` | pass |
| Server certificate: the unit credential `moonlight-server.pem`, equal to the sender's | pass |
| Focus after Start: the viewer window, fullscreen | pass |
| Status `Running`, snapshot `connected: true` with the viewer's own connection identity, background → `Backgrounded` with the shell focused, resume → `Running` | pass |
| Stop: the unit and its runtime directory are gone, status `Stopped` | pass |
| `@korri:viewer` disabled: the same selection starts Moonlight, with no runtime directory and no lifecycle arguments | pass |
| One treaty byte changed: refused with both hashes, Moonlight starts; with Moonlight disabled too, the refusal is the start's error | pass |

The proof does not exercise the federated path: the peer request that makes
korrid start a viewer in a real stream. The aka binary proof below and the
owner's one game after the autonomous proof (ADR 0063) cover it first.

Moonlight's unchanged adapter answered "viewer stop was not confirmed" to the
first stop request both times (`Moonlight stops that needed a second
request`). It reads the unit right after a `--no-block` stop and finds it
still deactivating. The test then waits for the unit to end and asks again.
The Korri viewer's cleanup waits for the unit to end instead.

## aka: the plugin's built viewer with `--viewer-package`

```sh
nix run .#stream-cadence -- view --sender root@aka --sender-fixture \
  --viewer root@aka --viewer-package PACKAGE --out DIR
```

`aka-before-468/results.json`: the plugin's viewer package
`/nix/store/gs16w5hn7ybazqisfh212gkz6qgn0msp-korri-viewer-0.1.0`, built from
this slice before `origin/main` gained the #468 viewer fixes. 2026-11-14
02:05 UTC. Five runs, all valid: `copied-product`, `x11_vaapi`,
`decode=vaapi`, no stream stall, no capture gap, 57.8 to 58.4 new frames per
second, CPU 0.011 to 0.018 s/s. Startup was 10.2 s in runs 1 and 2 and 0.74 to
0.80 s in runs 3 to 5. A game unit was live on aka throughout; the result
records it. Tctl was 61.5 to 65 °C in runs 1 to 4 and 78.6 to 96.0 °C in
run 5.

`aka-final/results.json`: the final package
`/nix/store/qir0f40m7z5hvjlvnk7rhhbbk4ldsr0d-korri-viewer-0.1.0`, the one in
the plugin at `1fb0c6d` and in the guest check above. 2026-11-14 04:55 UTC.
Two runs, both valid: `copied-product`, `x11_vaapi`, `decode=vaapi`, no
stream stall, no capture gap, 58.1 to 58.2 new frames per second, CPU 0.020
to 0.022 s/s, startup 786 ms. A game unit was live on aka. Tctl was 80.2 to
88.0 °C. The tool then waited 600 s for aka to cool below its 82 °C start
limit between runs, read 88.8 °C, and ended the session; both restore checks
pass. A first attempt at 04:42 UTC was refused at 85.5 °C before any fixture
ran; its restore checks pass too.

These are proofs that the plugin's binary streams, not comparisons: no
baseline ran beside them.
