# Stateful V4L2 M2M installed-viewer proof — #616

**2026-11-24: tabs6 picture pass. op6 not installed; no picture verdict.**

This is an ADR 0063 functional proof of the signed, installed Rust viewer.
It is not a full production-launch, controller, frame-pacing or performance
acceptance. The supervisor deferred korrid launch and controller routing to
land. The owner approved installation on tabs6 only.

| Device | Delivery | Picture | Audio | Production launch/input |
| --- | --- | --- | --- | --- |
| [tabs6](tabs6/README.md) | Signed archive, normal korrid import and exact approval | Rising readable picture IDs over 65.225 s; clean stop | Viewer linked to Speaker playback; ALSA RUNNING | Deferred to land |
| [op6](op6/README.md) | Not installed: owner approved tabs6 only | No verdict | Not run | Not run |

## Source and delivery

- Core viewer source: `0c520d5b3a562df37d5656dd560cc92e9ce3f46d`.
- Publisher source: `korri-os/plugins`, branch `proof/viewer-aarch64-616`,
  commit `bcc857f267927e860944364c8b4fb3e68f810ea7`.
- Tested publisher tag: `proof-viewer-v4l2m2m-616-bcc857f26792`.
- [Dry run 45497299588](https://github.com/korri-os/plugins/actions/runs/45497299588):
  lifecycle, x86_64, aarch64 and assemble all succeeded. See
  `dry-run-result.json` and `signed-artifact.json`.
- Installed package: `/nix/store/6ifslzp70aj95xfkzjrvginl47x7vjm2-korri-plugin`.
- Installed executable:
  `/nix/store/7pl3smh70n1wbkp7ayi6ird2dbif9bqf-korri-viewer-0.1.0/bin/korri-viewer`.
- Signed archive SHA-256:
  `623c4dbd7de9fac824cb5dd9fa13a2ececfa0f3c269b90314f1bac89a13534f2`.
- Owner-approved native digest, tabs6 only:
  `d89823b5226a203a95650745e70303b1d2ea852e8a0ec1db531ea1229d65636b`.

The documented `plugin-repository.yml` dry run ran from `main` with the
pinned source inputs. It produced the real bound publisher signature without
publishing a release or catalog. `plugin-publish.yml` did not run. No default
branch was pushed. No system generation was deployed by this work.

`download_ci.py` checked the source checkout, run, ARM job, artifact ZIP
SHA-256, revision and catalog archive SHA-256. `upload.py` used the existing
chunked archive RPC through the device's CLI socket. The owner reviewed and
approved the exact native digest through korrid. Import and enable succeeded
(`tabs6/install.log`); installed provenance says `signed: true`
(`tabs6/installed.json`). The first upload expired before installation; the
identical archive was restaged. No publisher binding, signature, permission,
approval, receipt or runner-selection check was bypassed.

`catalog.json` is dry-run artifact metadata, **not a published repository**.
Its release URLs are not shipment endpoints. The installed archive is an
approved sideload, not a public catalog selection. The existing official
repository setting was not changed. CI artifacts can expire.

## How the device proof ran

`picture.py` ran only the installed viewer, as the existing unprivileged
`korri` account, against aka's private sender fixture. It used the installed
plugin's `settings.ts` defaults and observed compatible bytes/display facts.
The ignored, effect-free test `emit_the_installed_handheld_proof_command`
called korrid's real `platform_from_path` and `stream_args`; the saved command
was not a hand-written approximation. No input seats were supplied, so the
production builder emitted `-viewonly`.

The proof requested fullscreen/focus only for the compositor window owned by
its viewer PID, using the existing compositor's control program. There was no
second compositor, sender, input
coordinator, raw uinput probe or federation test identity on the handheld.
Temporary Moonlight pairing keys and the fixture server certificate were
removed after the stream. The original output power state was restored.

The sender used its own ports, compositor, 1 kHz tone and motion source, not
product Sunshine. Aka had **no live product game at fixture begin**. Its
recorded product-unit PIDs and start times did not change. Source-scoped,
destination-scoped temporary TCP/UDP firewall rules admitted only this
fixture's port span from tabs6's observed LAN address. Exact rule deletion
and subsequent absence were checked. The private fixture directory and all
fixture processes were removed. See `picture-result.json` and compressed
sender evidence in `tabs6/`.

The handheld monitor refused starts at or above 75 °C on any thermal zone
and stopped at 85 °C. It recorded all 24 zones during the stream. Aka also
used the existing sender's separate heat guard and cooldown; its observed
cooldown was 87.875 °C to 75.750 °C before fixture preparation. Do not read
the handheld's 75/85 °C limits as measurements of the sender.

## Evidence limits

- Three readable frame IDs prove moving pictures across at least 60 seconds.
  They are not a presented-frame count or a pacing distribution.
- The carried stateful path wrote only the frame-record CSV header. Exact
  decoded/presented counts and presentation drops remain unavailable.
- The successful device log reports **62 network-dropped frames**. This is
  network loss, not a measured decoder or presenter drop count.
- Audio routing and an active hardware PCM are verified. No acoustic recording
  or human listening check was made.
- `tabs6/attempts/` retains the first private-port pairing timeout and a
  harness attempt with slow capture, missing fullscreen focus and wrapper
  teardown. The final run fixed only proof orchestration. It did not change
  the viewer artifact or carried C decode policy.
- Sender logs are gzip-compressed. Sunshine's transient stream AES key is
  redacted before retention. The JSON records both source and retained hashes.

## Checks

| Check | Result |
| --- | --- |
| Publisher publication/selection tests | 19 passed |
| Publisher scoped-input churn checks | x86_64 and aarch64 passed |
| ARM native/license/source check | Passed |
| Aarch64 QEMU loader/FFmpeg check | Passed; H.264 and HEVC `v4l2m2m` present |
| Production argument-builder tests | 16 passed, 1 manual proof test ignored; explicit proof invocation passed |
| Stream-cadence copy/source tests | 5 + 3 passed |
| Stream-cadence plugin-viewer tests | 54 passed |
| Proof helper tests | 11 passed |
| `ruff check clients/viewer/proof/v4l2m2m` (repository configuration) and Rust formatting | Passed |
| korrid Clippy with `-D warnings` | Blocked by 16 existing lints in acquisition/surface/game_assets; none in the added test |
| Final signed dry run | All four jobs succeeded |

The viewer lifecycle VM gate is explicitly x86_64-only because the hosted ARM
runner lacks KVM. The publisher branch documents and tests that choice. ARM
build, source/license/native checks and device picture proof remain required;
the ARM VM was not silently skipped as a successful ARM lifecycle test.

## Remaining shipment work

1. Land the Core and publisher commits through their normal review path.
2. Exercise the installed viewer through the production korrid launch and
   controller route after land, as deferred by the supervisor.
3. Obtain a separate owner decision before installing or testing op6. This
   record is not permission to install there.
4. Run the normal pinned publication workflow only when publication is
   authorized, then advance the official catalog/device selection and verify
   the deployed selection. Neither public publication nor an image seed was
   done here.
5. Measure frame pacing/counts and acoustic output separately if those are
   required for the next acceptance gate. This proof claims functional picture,
   not performance parity.
