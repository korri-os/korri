# webOS embedded-viewer media-link spike (partial)

Workstation only. No TV access, target execution (including QEMU), IPK,
installation, connection, pairing, identity files or protocol changes.

```sh
CARGO_BUILD_JOBS=2 nix run .#webos-embedded-viewer-spike-build
```

This Nix task cross-builds the pinned Moonlight TV v1.6.36 components:

- moonlight-common-c `c86e0537d11566f9825d6323711ec61e3e702365` and its ENet
  `115a10baa1d7f291ff5b870765610fd3b4a6e43c` gitlink;
- ss4s `9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2`, its core library and only
  its SMP webOS 3 module, using its legacy C++ ABI wrapper, AcbAPI resource and
  jail check;
- `media-link-probe`, which forces protocol and backend linkage. It neither
  initializes media nor plays a stream. The build never runs it.

The source archives and prebuilt webOS SDK are hash-pinned in `default.nix`.
Nix extracts/relocates its **own** SDK and patches only x86_64 host executable
loaders, using the existing `slint-spike/webos-cross.sh` technique. It does not
change a shared SDK or use TV libraries. C/C++ compilation runs at most two
jobs. There are no Nix build dispatches to a device.

`check_elf.py` inspects actual output ELFs with readelf: ARM32 EABI soft-float,
`/lib/ld-linux.so.3`, GLIBC at most 2.19, legacy GLIBCXX at most 3.4.20, no
RPATH/RUNPATH, no SDL/NDL dependency and no imported executable-memory patch.
Tests run the real reader on the ARM artifacts and corrupted temporary copies;
no ARM instruction is executed. The task prints the Nix output and its report.

**This is not the requested complete viewer CLI.** The spike isolates the
protocol/backend cross-link question. It does not implement GameStream HTTPS,
certificate provisioning, `/launch`/`/resume`, Opus/audio output, decoder
callbacks, play-time input, readiness/control IPC or korrid lifecycle
integration. In particular it does not reuse libgamestream's disabled TLS
verification to produce an apparently working but insecure CLI.

The SDK's vendor API libraries are link inputs only; they are not copied into
this output. The reported NEEDED SONAMEs and C++ exports must be checked against
the B7 jail before a runtime experiment. The output does not claim dependency
closure or B7 ABI compatibility merely because its own GLIBC symbol floor
passes. Redistributable crypto/audio dependencies also need a separate package
review. The SDK is glibc 2.12.2; the existing shell's TV runtime overlay is not
needed for this C/C++ link spike.

No media backend, schema, wire or product settings are enabled by this task.
The media module is built from upstream sources without copying them into
Korri; NDL's `m3_kadp_fix` is not compiled. Notices included in the output are
not a complete release source-compliance package. The source pins, license
findings, integration gates and ordered TV experiments are in
[`2026-11-07-webos-embedded-viewer-design.md`](../../docs/briefs/2026-11-07-webos-embedded-viewer-design.md).
