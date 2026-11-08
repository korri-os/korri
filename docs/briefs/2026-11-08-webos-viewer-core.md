# webOS embedded viewer core (2026-11-08)

**A linkable viewer core and a workstation CLI now build for ARMv7 soft-float.
This is not M4 acceptance. No TV or real streaming host was contacted.**

This implements the core of the [embedded-viewer design](2026-11-07-webos-embedded-viewer-design.md),
not its future exact-launch adapter. It does not change the launcher, appinfo,
IPK, korrid, any treaty, or the shell. The [milestones](2026-11-06-webos-milestones.md)
still require visible video, audio, input, and return to Zone on the TV.

## 1. Where the existing Linux flow runs

Verified against Korri base `7b9aec440e6f95a8a0dcfe3a07c0a3ea9f453bde`, the
[plugin producer at `57de061520cc010cbc5ebe9b978f4799d159affa`][plugin],
Moonlight Embedded `775444287305849ebdf4736c75298ad0713e2d5d` (the plugin's 2.7.1-korri base) sources, and the checked-in Android common-c sources.
The plugin's security patch changes Embedded's TLS behavior; unmodified
Moonlight TV libgamestream is **not** a safe substitute.

| Step | Existing implementation and responsibility |
| --- | --- |
| Select and prepare the real game | Rust [`UpstreamRegistry::prepare_stream`](../../services/korrid/src/upstreams.rs) keeps the exact far launch ID. It starts the viewer after preparation and compensates a failed viewer start against that same launch. The viewer does not select/authorize another game. |
| Obtain the client public cert | Rust [`ProductionLinuxStreamViewer::start` / `bootstrap`](../../services/korrid/src/linux_viewer/production.rs) invokes the plugin's [identity bootstrap][bootstrap] in a viewer sandbox. Only the bounded public certificate returns. The private key remains in the viewer keydir. |
| Discover Sunshine UUID | The same Rust start reads public HTTP `/serverinfo`, with a five-second deadline, no redirects, and a 64 KiB body limit. It validates `uniqueid`. This observation alone grants no authority. |
| Attest and provision | [`upstreams.rs::moonlight_native_peer` / `moonlight_certificate_provision`](../../services/korrid/src/upstreams.rs) asks authenticated native peers to attest the UUID and requires exactly one match. Existing `app.moonlight.certificate.provision` sends `hostUuid` and `clientCertificate`. The holding korrid's [`host/moonlight_certificate.rs`](../../services/korrid/src/host/moonlight_certificate.rs) uses the protected Sunshine Unix adapter. The [producer patch][sunshine] imports the certificate idempotently and returns the public server certificate. There is no PIN exchange. |
| Deliver the pin and start the transport | Rust [`host/viewer_units.rs`](../../services/korrid/src/host/viewer_units.rs) puts the server cert in a systemd credential. `KORRI_MOONLIGHT_SERVER_CERTIFICATE` identifies it. `stream_args` starts bundled Moonlight Embedded with `stream -app 'Korri Stream' -keydir …` and the selected address, not an external Moonlight app. |
| Authenticate and resolve the transport app | Embedded [`gs_init` / `load_serverinfo` / `gs_applist`][embedded-client] use client-cert HTTPS and resolve `Korri Stream` through the authenticated app list. The plugin's [pinning patch documentation][pin] requires peer verification, the supplied cert as trust anchor, and an SPKI pin. Hostname matching is not Sunshine's identity mechanism. Rust does not perform this HTTPS app-list/launch work. |
| Launch or resume | Embedded [`gs_start_app`][embedded-client] sends authenticated HTTPS `/launch` or `/resume`, with the transport app ID, mode, fresh `rikey` and `rikeyid`, audio/controller parameters, and common-c's query extensions. It reads `gamesession`/`resume` and `sessionUrl0`. This is the Sunshine transport, not the selected Korri game's launch authority. |
| RTSP, stream, decoding | Embedded [`src/main.c::stream`][embedded-main] calls `LiStartConnection`. [`common-c Connection.c`](../../clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src/Connection.c) owns startup/teardown stages; [`RtspConnection.c`](../../clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src/RtspConnection.c) performs RTSP negotiation. common-c then handles control, encrypted input, UDP media, FEC, and depacketization. The viewer supplies decoder/audio callbacks and input. common-c is not the HTTPS control client or decoder. |
| End or leave | Embedded calls `LiStopConnection`; stock `-quitappafter` separately calls HTTPS `/cancel`. The [Korri plugin excludes `quitappafter` and PIN settings][settings]: korrid owns game lifetime/trust. Rust [`stream_play/device.rs::spawn_watch`](../../services/korrid/src/stream_play/device.rs) polls the exact far session. [`session_status_resolved`, `recover_viewer`, and session control in upstreams.rs](../../services/korrid/src/upstreams.rs) own stop/recovery/freeze/thaw. A local viewer exit is not proof that the far game ended. |

Linux Home is the exact session-leave/overlay path, not Moonlight's quit chord.
`app.session.leave` freezes/backgrounds; End ends the exact session. A stopped
viewer can be restarted by `recover_viewer` while its far launch stays selected.
A future TV adapter must order intentional leave before exposing a stopped
viewer. This core deliberately has **no `/cancel`, pairing endpoint, PIN, peer
RPC, host database, or independent game-end authority**.

Android confirms this division: [`KorriStreamViewer.java`](../../clients/android/app/src/main/java/com/limelight/KorriStreamViewer.java)
prepares/provisions the identity and resolves one transport app, then delegates
to the embedded Activity. [`NvHTTP.java`](../../clients/android/app/src/main/java/com/limelight/nvstream/http/NvHTTP.java)
provides the existing GameStream request parameter names and launch/resume
responses used here. No new Korri wire fields are needed for this core.

## 2. What is built

Implementation: [`clients/webos-viewer/`](../../clients/webos-viewer/).
[`viewer.h`](../../clients/webos-viewer/viewer.h) is the C library boundary.
`cli.c` is a separate test program, not the application's launcher/main.

| Artifact/dependency | Grounding |
| --- | --- |
| `libkorri-webos-viewer.a` | Identity, bounded HTTP/TLS, GameStream parsing/launch/resume, common-c callbacks, Opus stereo decode and ss4s delivery. |
| `libmoonlight-common-c.a`, `libenet.a` | **The exact checked-in Android fork**, not a new Moonlight TV gitlink: [Android README](../../clients/android/README.md) records origin `ClassicOldSong/moonlight-common-c` at `c9994368`. The current vendored tree is `ffdb41fabcaf5f4684b992314e5ad25d8d14d9e5`, including Korri's later changes and bundled ENet/Reed-Solomon. Both platforms now consume those same bytes. No generic/system ENet substitution. |
| `libmbedtls.a`, `libmbedx509.a`, `libmbedcrypto.a` | mbedTLS **3.6.7**, archive SHA256 `a7e8bcbec0e6f761b4af24f25677626b35f762f68eef79c08677a363212d11f6`. TLS, identity and common-c crypto all link statically. No dependency on TV OpenSSL 1.0.0 or a workstation `libcrypto.so.3`. |
| `libopus.a`, `libexpat.a` | Opus **1.6.1** and Expat **2.8.4**, source pins inherited from the repository's locked nixpkgs. Opus uses fixed-point decoding and disables architecture intrinsics. Expat rejects DTDs before expansion. |
| `libss4s.so`, `ss4s-ndl-webos4.so`, `ss4s-pulse.so` | ss4s **`9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2`**, from the previously inspected Moonlight TV tag. Only `ndl-webos4` video and Pulse PCM audio are built for TV. No NDL audio selection, SMP, SDL, launcher UI, power policy or joystick enumeration. |
| `webos-viewer-test` | Links the same core archive. Native tests build `ss4s-dummy.so` instead of the two TV modules. The ARM CLI rejects the dummy mode. |
| Cross SDK | Existing openlgtv prebuilt **2026.08-webos** SDK, archive SHA256 `2230a6351e5cce6fcb9d4fdcb4697335f223995401ef969c81744aaeb14c5344`. Private Nix extraction; only its x86-64 tool loaders are patched. SDK libc is not bundled. |

The build uses two compiler jobs, on the workstation only. The kernel CSPRNG
uses raw `getrandom` syscalls, not a glibc wrapper newer than the TV and not
`/dev/random`. The SDK predates the ARM syscall header entry, so the build
supplies ARM `SYS_getrandom=384`. mbedTLS's hardware-entropy hook and Expat use
that kernel source. Failure does not fall back to a weak seed.

Two narrow ss4s corrections are in [`ss4s.patch`](../../clients/webos-viewer/ss4s.patch):
video-close releases the player mutex before calling the backend, because
closing a decoder can flush a frame report that takes that mutex; and Pulse
allocation/write errors now propagate instead of reporting successful audio.
Native tests configure the real dummy driver to report a final frame at close,
and an unpatched-core reproduction timed out (exit 124) after five seconds;
the corrected core exited 0 with the same driver. That test-only patch
is not applied to TV modules. The NDL jail guard and original KADP helper are
retained; the prior TV evidence says the helper is a no-op on m16p, not that it
is safe to enable on every other SoC.

### Identity and trust invariants

The caller supplies the directory. The TV allocation, approved by the owner,
is `/media/developer/org.korri.shell/moonlight/`. There is no default identity
path in the library and no additional persisted schema.

- Exactly `client.pem`, `key.pem`, `uniqueid.dat`. RSA-2048; self-signed
  SHA256 X.509, subject `CN=NVIDIA GameStream Client`; unencrypted PKCS#8 PEM;
  16 hex characters with no trailing newline. Format grounding is Embedded's
  [`client.c`][embedded-client] and [`mkcert.c`][embedded-mkcert]. No PKCS#12,
  host list, pin file, session record or copied federation private key is made.
- Descriptor-relative ancestor traversal refuses symlinks and `..`. The owned
  directory has an advisory lock until `kv_identity_free`. The full allocation
  must contain only the three caller-owned, singly-linked regular files.
  Unknown entries, foreign ownership, hard links, symlinks and partial/corrupt
  identities fail closed. A corrupt identity is never silently rotated.
- After the audit, held directory/file descriptors re-tighten modes to
  **0700/0600**, including after devmode's boot-time `chmod -R 777`. Replaced
  names cannot make chmod follow another path. Missing parent directories are
  not created; korrid's allocation must already exist.
- `identity DIR` writes **only the public certificate** to stdout. The key
  stays in the viewer. The future korrid adapter receives only that public
  certificate for existing provisioning.
- The supplied broker-returned server certificate is the only TLS anchor.
  Verification is required, verification flags are never cleared, and the
  leaf DER must equal the supplied certificate. TLS proves possession of its
  private key. Expiration checks remain enabled. An unrelated certificate or
  even a different certificate with the same public key is refused.
- DNS names are not Sunshine identities. `mbedtls_ssl_set_hostname(NULL)`
  explicitly selects certificate-pinned identity, as opposed to turning off
  peer verification. Routes are numeric IPv4/IPv6. No DNS, system CA store,
  proxy, redirects or HTTP fallback for authenticated calls.
- Responses are bounded to **128 KiB**, headers to **8 KiB**, with a single
  **10-second** connect/TLS/write/read deadline. Unknown compression/chunking,
  duplicate lengths, truncation, invalid status/XML, DTDs, duplicate recognized
  XML fields and excessive nesting fail closed. Sunshine's ordinary
  Content-Length framing is supported; this is not a general-purpose web client.
- Request/session/key material is not logged. common-c and ss4s diagnostics are
  suppressed in the streaming path. The CLI disables core dumps.

This replaces, rather than adapts, Moonlight TV libgamestream's unverified cURL
path. Its [pinned `http.c`][tv-http] disabled peer/hostname checks, grew buffers
without a bound and could log secret-bearing URLs. Linking common-c alone
would not correct those defects.

## 3. Running the workstation checks and CLI

From the repository root, one expensive command at a time:

```sh
CARGO_BUILD_JOBS=2 nix run .#webos-viewer-check
CARGO_BUILD_JOBS=2 KORRI_WEBOS_TV_LIBS="$HOME/.cache/research/korri-webos-m2/evidence/tvlibs" \
  nix run .#webos-viewer-build
```

Each command prints its Nix output directory. The native output needs
`LD_LIBRARY_PATH="$native/lib"` for its CLI and dummy module; the eventual app
launcher owns the equivalent module/library search path on TV. This change
does not package those files or invent a private process-control protocol.

CLI forms (arguments are explicit; these lines are not instructions to contact
any existing host):

```text
webos-viewer-test identity ABSOLUTE_DIR
webos-viewer-test info DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM
webos-viewer-test launch DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM APP_ID dummy SECONDS
webos-viewer-test resume DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM APP_ID dummy SECONDS
webos-viewer-test launch-check DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM APP_ID
webos-viewer-test resume-check DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM APP_ID
```

`SERVER_CERT_PEM` must be the existing broker's returned public cert, **not**
a TOFU capture from the target address. `info` uses authenticated HTTPS.
`launch`/`resume` first repeat authenticated serverinfo, require paired state,
and refuse an occupied different transport. Baseline is H.264 1920×1080 at
60 fps, 10 Mbit/s, stereo PCM after Opus decode, and common-c's encryption
flags. These are test-CLI parameters, not persisted launch settings. No pads
are advertised or forwarded yet.

`dummy` is workstation-only; the ARM program accepts `ndl` instead. Normal
streaming ends on SIGINT/SIGTERM, configured duration, connection termination,
media failure or no accepted video within 15 seconds after connection startup.
`LiStopConnection` runs on the owning control thread, never inside a callback.
Successful return requires both connection-started and an accepted decode
unit. This is **not a visible-frame readiness claim**. `*-check` stops after
HTTPS launch/resume and deliberately does **not** run RTSP; it can leave a
transport running and is for isolated test servers, not real-host operation.
No command ends the selected far Korri game.

## 4. Verified results and limits

| Check actually run | Result |
| --- | --- |
| Native Nix build and real CLI/filesystem/loopback-TLS tests | **11 passed**: stock PEM identity/persistence/modes; no partial/corrupt rotation; key mismatch; symlink/hardlink/foreign directory refusal; directory locking; required client certificate; exact server pin; same-key reissued certificate refusal; expired pin refusal; authenticated launch/resume parameters; occupied transport refusal; response/XML bounds. |
| Native C protocol/media test | Pass: serverinfo and resume/session parsing, malformed/rejected XML, and two real ss4s dummy open/feed/close cycles, including frame reporting during close. It does not decode H.264 or prove NDL. |
| ARM cross build and final link | Pass: the core and all static dependencies link into the CLI; video and audio modules link. No target code executed. |
| Four output ELFs | ARM32, little-endian, EABI5 soft-float calling ABI; CLI interpreter `/lib/ld-linux.so.3`; maximum GLIBC requirement **2.12**; no RPATH/RUNPATH or Nix-store dynamic paths. |
| Symbol resolution against original `tvlibs/` | Pass for all four ELFs: required imports resolve against supplied system-library evidence plus the output. Weak UND hooks are optional. **Zero-valued GLOBAL/WEAK ABS exports are rejected**, not patched to zero; no such exports are present. No TV library is bundled. |
| Real-ELF audit regression tests | **10 passed**: valid artifacts, corrupted machine/hard-float, missing module/SONAME, new GLIBC version, unresolved imports, weak UND behavior, and GLOBAL/WEAK zero-ABS rejection. |
| Type/lint/format checks | GCC strict warnings (`-Wall -Wextra -Werror -Wconversion -Wshadow`) on core/CLI in both builds; Ruff check/format; strict mypy on audit tools/tests and checked test bodies; clang-format and Nixfmt checks; `git diff --check`. |
| Local Sunshine / RTSP / hardware | **Not tested**. No `sunshine` program was available on PATH. No real host, including haku, was contacted. Loopback TLS servers and protocol fixtures replace that unavailable test; they do not establish Sunshine interoperability or a completed RTSP session. |

The audit prints SHA256s of the original provider files and each output's
required/optional import counts. Providers are libc, pthread, rt, m, dl,
NDL directmedia, Pulse and Pulse-simple. The CLI has no dynamic OpenSSL,
mbedTLS, Opus, Expat or C++ runtime dependency.

No full Rust/project test suite was run; production Rust is unchanged. The
upstream Opus build emits GCC `-Wstringop-overflow` diagnostics in its SILK
code. They are not warnings in the new core; they remain an upstream review
item, not evidence of a tested decoder failure.

## 5. Integration and TV work still required

1. **Link into the approved multi-call main.** Consume the core archive plus
   its static dependency archives and `libss4s.so`; load only the two TV
   modules. The public header includes `Limelight.h`. CMake lists the exact
   link dependencies. The independent worker owns `bin/korri-webos`, launcher,
   appinfo and IPK changes. The NDL path refuses root. NDL must execute in that main-path process with
   the app's LS2 identity and `APPID=org.korri.shell`. This standalone test ELF
   is not claimed to have that identity on the TV.
2. **Reuse the existing Rust broker and exact-launch authority.** Compose
   through the embedded viewer seam, bootstrap only the public certificate,
   read public serverinfo, attest/provision, then authenticate. Resolve exactly
   one positive `Korri Stream` app ID via the authenticated app list before
   invoking the core. This CLI takes an explicit app ID; an authenticated
   app-list resolver is **not implemented** here. Do not persist/hardcode an ID.
3. **Lifecycle is not yet the exact viewer adapter.** The library is blocking
   and common-c/ss4s are process-global; only one stream can run at a time.
   It reports no cross-process readiness or input messages. Common-c's
   synchronous startup and vendor/Pulse calls still need bounded cancellation
   and shutdown measurement in the chosen main-mode lifecycle. An in-process
   vendor hang cannot be made safe by an HTTP deadline. Define the actual
   readiness observation before returning `Running` from the korrid trait.
4. **Preserve game authority.** Confirm intentional Leave/End ordering and
   stopped-viewer recovery with regression tests before wiring the watcher.
   Unexpected-disconnect policy is still an owner decision. Do not turn local
   transport failure into `/cancel`, nor silently call it a successful game end.
5. **TV identity test, with approved operator access only.** As the app UID,
   create the approved folder, verify restart persistence, permissions after
   boot chmod and refusal of altered allocations. Never use another app's
   Moonlight keys. Test wrong-pin refusal before any real-host operation.
6. **TV playback, after the owner authorizes that real host.** Keep the shell's
   original graphics process/window. Verify client-cert HTTPS and exact app
   resolution; then RTSP, first accepted decode unit, visible H.264, audible
   Pulse stereo, latency, CPU/RSS, close and second open. Accepted NDL buffers
   are not proof of a visible image; Zone remains opaque in this change.
7. **Input and overlay remain separate.** No evdev, raw remote forwarding,
   controller seats or Home/Back mapping is added. Use korrid's single native
   input producer and obtain approval before any new input/readiness wire.
   A design engineer owns see-through composition and overlay presentation.
8. **Distribution remains blocked on the linkage/license plan.** common-c and
   this core are GPL-covered; ss4s is LGPLv3. The output includes component,
   ENet and Reed-Solomon notices, but that is not a complete Corresponding
   Source/relinking delivery. Preserve exact sources, patches, locked build
   inputs and build instructions. Review linkage with Slint and proprietary
   LG APIs, source delivery and LGPL relinking before distributing an IPK.
   Do not bundle SDK/vendor runtimes to satisfy missing symbols.

[plugin]: https://github.com/korri-os/plugins/tree/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight
[bootstrap]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight/identity-bootstrap.py
[pin]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight/README.md#server-certificate-pinning-new-korri-security-patch
[settings]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight/README.md#options-with-no-typed-setting
[sunshine]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/services/sunshine/patches/0020-add-korrid-certificate-control.patch
[embedded-client]: https://github.com/moonlight-stream/moonlight-embedded/blob/775444287305849ebdf4736c75298ad0713e2d5d/libgamestream/client.c
[embedded-mkcert]: https://github.com/moonlight-stream/moonlight-embedded/blob/775444287305849ebdf4736c75298ad0713e2d5d/libgamestream/mkcert.c
[embedded-main]: https://github.com/moonlight-stream/moonlight-embedded/blob/775444287305849ebdf4736c75298ad0713e2d5d/src/main.c
[tv-http]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/http.c
