# webOS M1: the real shell over local sample data

These tasks run only on the workstation. They never contact, install on, or
launch anything on a TV. The parent operator owns every TV action.

## Build

Use a clean, committed checkout. Prepare the SDK as described in
`../../slint-spike/WEBOS-CROSS.md` and apply the checked public-runtime overlay
from `../../slint-spike/WEBOS-RUNTIME-OVERLAY.md`. Neither the SDK archive nor
any firmware library belongs in Git or the IPK.

```sh
prep="$HOME/.cache/research/korri-lg-b7-prep-20261029T175543Z/readiness"
clients/slint-spike/webos-sdk.sh \
  "$prep/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz" "$PWD/.cache/webos-sdk"
# Prepares SDK host executables and checks the soft-float C ABI; no target execution.
nix run .#slint-spike-webos-probe
clients/slint-spike/webos-runtime-overlay.sh "$PWD/.cache/webos-sdk" \
  "$prep/tv-runtime-glibc-2.19" \
  "$prep/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz" "$PWD/.cache/webos-target"
source_sha=$(git rev-parse HEAD)
test "$(git rev-parse HEAD)" = "$source_sha"
nix run .#shell-webos-build
nix run .#shell-webos-proof -- "$PWD/.cache/webos-proof"
nix run .#shell-check
nix run .#shell-webos-package -- "$PWD/.cache/webos-ipk" --source-sha "$source_sha"
nix run .#shell-webos-package-check -- --check-ipk \
  "$PWD/.cache/webos-ipk/org.korri.shell_0.0.0_arm.ipk"
```

Preparation runs once per private SDK extraction; the overlay refuses a second
application. `WEBOS_SDK` can select an already prepared SDK. The shell ARM cache
is `.cache/shell-webos-target`; the workstation SDL cache is separate. The ARM
task uses at most two jobs and records the source SHA plus both binary hashes.
Packaging refuses changed binaries, a dirty checkout or a different source SHA.
Use a new output directory for each package.

`package.py` promotes no new runtime schema. The shell's existing `install`
command verifies Zone and Pico on the workstation. Their cache entries use
relative paths (`packages/zone`, `packages/pico`) and zero mtimes that the tar
payload preserves. The TV runs `--installed`, which checks these file stamps
and reads the summaries; it never runs install-time verification there.
All existing surface package files, fonts and licenses are included unchanged.

## Launch contract

The new native application id is `org.korri.shell`, with `launch.sh` as `main`.
The launcher ignores AppManager's optional launch JSON. It must already run
unprivileged in the native jail and have a writable private `HOME`; it refuses
UID 0. The B7 spike ran as UID 6269, GID 5000, with no effective capabilities
(parent's observed process, 2026-10-30). No permission, service, startup hook,
SDL library, firmware library or real korrid is installed by this IPK.

The launcher changes to its app directory and creates a fresh mode-0700
`$HOME/korri-shell.XXXXXXXX` directory. It starts the fixture korrid with twelve
built-in games, an ephemeral loopback port, and stdin closed. The fixture keeps
serving after EOF. It generates a 256-bit kernel CSPRNG capability and writes
only `credentials/KORRID_RPC_CAPABILITY`, mode 0600. Core dumps are disabled.
The capability is never an argument, URL, environment variable, package entry
or log line. The temporary binding file contains only address and Origin.

The launcher reads the fixture's two public stdout lines without executing
them and exports `KORRID_ADDRESS`, `KORRID_SHELL_ORIGIN` and
`CREDENTIALS_DIRECTORY`. It also sets `APPID=org.korri.shell` (overriding the
adapter's historical spike default) and `SLINT_SCALE_FACTOR=1.5`, the adapter's
existing scale for a physical 1920x1080 output. It starts the real shell with
`--packages packages --cache cache --installed --surfaces zone,pico
--placement fullscreen`. Zone starts first. F2 or the existing L1+R1 chord
switches to Pico and back. No TV-remote switch mapping is invented here.
Exit and HUP/INT/TERM kill and reap both children and remove that launch's
private directory. SIGKILL/power loss can leave a stale directory, but the next
launch never reads or reuses it. Fixture preferences are intentionally transient.

The proof task uses the same launcher, verified packages, environment contract
and `webos` feature on x86_64 SDL2 under a 1920x1080 Xvfb display. It captures
Zone, F2 to Pico, and F2 back to Zone, then checks launcher cleanup. The ARM
binaries are never executed by these tasks.

## Compatibility limits

The target is `armv7-unknown-linux-gnueabi`, ARM EABI5 soft-float calling ABI,
`/lib/ld-linux.so.3`. The SDK's original glibc 2.12 is too old for the current
Rust standard library. The local-only TV glibc 2.19 overlay supplies `getauxval`
and other real target symbols. Both ARM ELFs must need no GLIBC version above
2.19, and must contain no Nix store dynamic reference. The fixed getrandom
`linux_raw` backend uses kernel entropy, not a missing glibc wrapper or jailed
`/dev/random`. `package-check` checks the actual archived binaries too.

Slint's interpreter and QuickJS are cross-compiled, not replaced by a mock UI.
The webOS feature enables rquickjs's supported `bindgen` feature (as korrid
already does): rquickjs 0.14's bundled ARMv7 bindings have the obsolete
six-argument ArrayBuffer signature, but its actual QuickJS uses seven.
The SDK target headers and soft-float bindgen flags generate the correct ABI.
Their TV execution, memory demand and input must still be checked on the TV.
The renderer is Slint software rasterization, uploaded as a dirty rectangle to
an SDL streaming texture. The adapter requests an accelerated SDL renderer
first (GLES2 on the B7), then accepts another SDL renderer if unavailable.
It logs the selected renderer. Desktop success does not prove TV performance.

Surface fonts are bundled. System fontconfig and fallback fonts are not bundled;
missing TV fontconfig or coverage may still affect text not covered by the
package fonts. The system LG SDL 2.0.4 is loaded at runtime. Do not substitute
the bundled SDL-webOS build that froze this TV; see the 2026-10-30 input report.

The parent diagnosed the later system-SDL hang on 2026-11-06: webOS IPK
upgrades leave removed payload files behind. A stale
`lib/backports/libSDL2-2.0.so.0` from the bundled build was still being loaded.
Removing it let the old app reach the foreground in 3 seconds using the
system SDL. This package has no `lib` or `lib/backports` directory. Its id is
new, so the parent's first installation must be fresh. For every subsequent
test, **uninstall `org.korri.shell` first**, then install this IPK. An in-place
upgrade is not a clean deployment and is not an accepted test path. The
parent owns both operations and must verify the installed payload list or
fresh-install result. Do not uninstall or alter `org.korri.slint-spike` as part
of these workstation tasks.
