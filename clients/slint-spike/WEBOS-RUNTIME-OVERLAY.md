# Private SDK runtime overlay (throwaway experiment)

The SDK sysroot is glibc 2.12.2. The measured TV is glibc 2.19. The actual
stripped-release korrid executable link failed on `getauxval` from ring/libstd.
`getauxval` is a real glibc export from 2.16, not a symbol to replace with a shim.
Separately, `renameat2` needs the existing kernel syscall boundary because its
libc wrapper arrived in glibc 2.28, later than both SDK and TV.

The parent owns a read-only fetch of eight **public** TV `/lib` SONAME files:
`libc.so.6`, `ld-linux.so.3`, `libpthread.so.0`, `libm.so.6`, `libdl.so.2`,
`librt.so.1`, `libutil.so.1`, and `libresolv.so.2`. The parent verifies each
against its TV checksum and writes a completed `SHA256SUMS`. No firmware
library belongs in Git, an IPK, or the app's private runtime directory.

Only after the parent confirms that fetch completed:

```sh
prep="$HOME/.cache/research/korri-lg-b7-prep-20261029T175543Z"
clients/slint-spike/webos-runtime-overlay.sh \
  "$PWD/.cache/webos-sdk" \
  "$prep/readiness/tv-runtime-glibc-2.19" \
  "$prep/readiness/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz" \
  "$PWD/.cache/webos-target"
```

Pass the prepared SDK and its target cache explicitly when running from a
combined checkout. The fourth argument is the cache protected by the build's
`.writer.lock`; a symlink to the existing cache uses that same lock.

This local Nix-shebang script:

- Verifies the original SDK archive digest, without modifying the archive.
- Requires all eight manifest entries, rejects extras/duplicates, and checks
  checksums on the regular input files and their staged copies.
- Requires ARM ELF32 EABI5 soft-float files with matching SONAMEs and an actual
  `getauxval@@GLIBC_2.16` export from libc.
- Takes the existing writer lock. It does not run a build or any ARM program.
- Saves dereferenced copies of all eight original SDK runtime files plus
  checksums in `SDK_DIR/.korri-runtime-original` before replacing any file.
- Unlinks each destination SONAME before moving its checked regular file into
  `SDK_DIR/arm-webos-linux-gnueabi/sysroot/lib`. It never writes through the old
  versioned symlinks. The SDK's original versioned files stay unchanged.
- Leaves SDK headers, startup objects, nonshared archives, and all host tools
  unchanged. The target linker scripts already refer to these `/lib` SONAMEs.
- Records the applied manifest as `SDK_DIR/.korri-tv-runtime-SHA256SUMS`.

This is a one-off operation. An existing backup refuses a second application.
If a filesystem error interrupts replacement, do not build against that partial
sysroot. Recover from the verified original archive or the complete original
runtime backup. The backup and writer lock make the local change reversible;
there is no on-device operation to reverse.

## Checks completed without TV libraries

- Core boundary: three real-filesystem tests passed, including two concurrent
  `RENAME_NOREPLACE` publications with exactly one winner. Focused Clippy
  `-D warnings` and rustfmt passed.
- The actual rename boundary compiled for `armv7-unknown-linux-gnueabi`.
  Its ARM object imports `syscall`, not `renameat2`.
- Overlay script: ShellCheck and bash syntax passed. Empty manifests were
  rejected. The eight real SDK 2.12.2 libraries passed ELF/SONAME validation but
  were rejected for missing `getauxval`. All eight original runtime checksums
  stayed unchanged, and no backup/application occurred.

The actual glibc 2.19 overlay and final executable link remain unverified until
the parent's matching public-library fetch is available. This does not claim
that mixed old SDK headers/nonshared objects and TV shared libraries establish
full runtime compatibility. The parent must inspect the final linked ELF and
test the embedded inspector on the unchanged TV runtime.
