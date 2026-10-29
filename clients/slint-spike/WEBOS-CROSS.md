# THROWAWAY SPIKE: off-device webOS ARM build

This is a workstation-only experiment. These tasks never contact a device and
never execute ARM output. They do not install an app or change a system library.
The parent experiment owns packaging, target testing, and the flake index wiring.

## Grounding

- Native prototype: `8f8d982505dec06f7d19a29d524650d0bf03de20`.
- Native code/evidence source: `14fa8068907a7158f52d9ebaf3eee5c5c25b11d5`.
- Real TV preflight: LG OLED65B7A-U, Linux 4.4.3, ARMv7, ELF32 little-endian
  EABI5, soft-float calling ABI, `/lib/ld-linux.so.3`, glibc 2.19.
- Prebuilt SDK: openlgtv/buildroot-nc4 release `2026.08-webos`, asset
  `arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz`, 365097814 bytes.
- Archive SHA-256:
  `2230a6351e5cce6fcb9d4fdcb4697335f223995401ef969c81744aaeb14c5344`.
- The archive's inspected sysroot is **glibc 2.12.2**, not 2.19. GCC is 16.2.0;
  its default ABI is `-mfloat-abi=softfp`. Rust target is
  `armv7-unknown-linux-gnueabi`, not `gnueabihf`.

The SDK is older than Rust's supported GNU/Linux glibc baseline (2.17). A tiny C
program does not prove that current Rust or the complete korrid will link.
Inspect the actual final ELF. Do not replace the TV libc or its loader to make
an artifact run. No private libc bundle is supplied here.

## Prepare

The existing verified archive is in the local prep pack:

```sh
prep="$HOME/.cache/research/korri-lg-b7-prep-20261029T175543Z"
clients/slint-spike/webos-sdk.sh \
  "$prep/readiness/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz" \
  "$PWD/.cache/webos-sdk"
```

`webos-sdk.sh` is a local Nix-shebang script. It refuses an existing destination,
verifies the checksum before extraction, and runs Buildroot's relocation script.
It does not require a working FHS host loader. The Nix cross task then changes
**only x86_64 host executable** interpreters and RPATHs in that private SDK copy
to use Nix libraries. ARM files are not patched. The SDK's original host RPATH
is retained after the Nix host libraries.

Default SDK: `$KORRI_ROOT/.cache/webos-sdk`. Override with `WEBOS_SDK` to reuse
an already relocated SDK. Only one task should prepare a shared SDK at a time.
Do not move a relocated SDK without rerunning its own relocation script.

## Tasks

`webos.nix` takes `{ pkgs, proseql }` and returns `apps` and `help`. Compose it
in the flake index like `default.nix`; no derivation belongs in `flake.nix`.
After that wiring:

```sh
nix run .#slint-spike-webos-probe
nix run .#slint-spike-webos-korrid
# Optional additional executable-link diagnostic, reuses the same artifacts:
nix run .#slint-spike-webos-korrid -- --bin korrid
# Requires the separate backend adapter's `webos` Cargo feature:
nix run .#slint-spike-webos-build
```

The probe compiles the checked-in `webos-abi-probe.c`. It checks the actual ELF
header, ARM attributes, interpreter, and version needs with SDK readelf. It
writes `.cache/webos-target/abi-probe/hello` and `readelf.txt`. Expected stdout
if the parent later elects to execute it: `off-device ARM ABI probe` plus newline.
The tasks themselves never execute that file, including under QEMU.

The korrid task builds the real library with its unmodified dependency set,
including the QuickJS sandbox, OXC, permission checks and cryptography. It
produces both the rlib and the manifest's cdylib. Rust-only entry points can be
discarded from the cdylib, so this is not proof of full executable linkage.
Use the `--bin korrid` diagnostic or the final spike to prove that link. The
diagnostic uses Cargo's dev profile to reuse the
initial compatibility build. The final spike task builds **stripped release**
with `--no-default-features --features webos`.

All cross output lives in `.cache/webos-target`. An exclusive `flock` rejects
concurrent writers. `CARGO_BUILD_JOBS` accepts only 1 or 2 (default 2).
Do not point another Cargo command at this target directory while a task runs.
No task writes `.cache/korrid-target` or another worktree's build output.

The Nix task supplies Rust's ARM standard library, native libclang and the host
C toolchain. Target-specific Cargo/cc-rs variables select the SDK compiler,
C++ compiler and archiver. Target-specific bindgen flags select the SDK headers
and softfp ABI. Target-specific pkg-config variables restrict lookup to the
SDK sysroot; host build dependencies still use native tools. Proseql is hydrated
through `services/korrid/proseql-source.nix`, not copied or replaced.

The final binary is expected at:
`.cache/webos-target/armv7-unknown-linux-gnueabi/release/korri-slint-spike`.
It still needs static inspection and the parent's clean pinned-source packaging
checks. Do not distribute a dirty build; `KORRI_BUILD_ID` labels it `-dirty`.

## Verification limits

C ABI probe: verified ELF32 ARM EABI5 soft-float, `/lib/ld-linux.so.3`,
`GLIBC_2.4` requirements. The first 7876-byte probe's SHA-256 was
`4a9c0866d0abedb325a2a45fe179c36f45a684a7d0d498951b9f3d24e8825962`.
The parent reported that exact file ran successfully on the TV. That is not a
claim that korrid, graphics, input, memory use, or the app lifecycle works there.

Complete korrid library compilation passed after the portability fix. Its
4.2 MiB dev cdylib has the correct soft-float ABI, only GLIBC_2.4 version needs,
and no RPATH/RUNPATH. This is not the final embedded application.

The first complete korrid cross attempt exposed a real ARM32 portability error:
`timeval.tv_usec` is i32, not LP64's i64. The narrow checked-conversion fix in
`federation/interfaces/routes.rs` retains the zero-duration refusal and 1 us
minimum. Its focused test uses an actual netlink socket, reads back SO_RCVTIMEO,
and verifies an empty receive times out. No permission or sandbox is disabled.
