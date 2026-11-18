# Vendored network core: nightjar-core (#391 step E)

The viewer's second network core, behind the `nightjar` cargo feature
(`src/network_core/nightjar.rs`). Both trees are GPL-3.0-only (ADR 0059:
the viewer and every binary that links it are GPL-3).

| Directory | Upstream | Pinned commit | Licence |
| --- | --- | --- | --- |
| `nightjar/` | <https://github.com/RamazanKara/nightjar> | `7faa84f7d2ee86ad0b5b39dd67677c42aaedc8cf` (2026-11-16) | `nightjar/LICENSE.txt`, GPL-3.0; `license = "GPL-3.0-only"` in `protocol/Cargo.toml` |
| `rubylight-native-api/` | <https://github.com/RamazanKara/Rubylight> | `17fc45bb7ab4c7528d4f533916f4ea79eb9653a3`, the revision that nightjar-core's `Cargo.toml` names | `rubylight-native-api/LICENSE`, GPL-3.0; `license = "GPL-3.0-only"` in its `Cargo.toml` |

What was copied, with `git archive` of the pinned commit, so the files are
the commit's and not a working tree's:

- nightjar: `LICENSE.txt`, and from `protocol/`: the workspace `Cargo.toml`,
  its `Cargo.lock` and `.cargo/config.toml`, `client/` (the crate
  `nightjar-core`), `core/` (`rubylight-protocol`), `traverse/`
  (`rubylight-traverse`) and `vendor/rusty_enet/` (Nightjar's patched
  `rusty_enet` 0.4.0, MIT, with its `LICENSE`). These are nightjar-core and
  every path dependency it has. Nightjar's apps, its C differential test
  harness (`protocol/difftest`, which needs Nightjar's patched
  moonlight-common-c submodule) and its other crates are not copied.
- Rubylight: `LICENSE` and `rust/core/native-api/` (the crate
  `rubylight-native-api`).

The first commit that holds these trees changes nothing in them. Korri's
edits come after it, and each is listed here:

| File | Edit | Why |
| --- | --- | --- |
| `nightjar/protocol/client/Cargo.toml` | `rubylight-native-api` comes from `../../../rubylight-native-api/rust/core/native-api` instead of Rubylight's git repository at the same revision | Nix builds offline from `Cargo.lock`. A git dependency would need its own fixed hash in each of the viewer's Nix builds, Android's too, and cargo fetches Rubylight with its submodules to resolve it even under a `[patch]` |
| `nightjar/protocol/client/src/control/session.rs` | `ControlSession` counts the IDR requests and the reference frame invalidations that `poll` queues for the host (`idr_requests_sent`, `rfi_requests_sent`). Each change carries a `Korri` comment: the two fields, their start at 0, the two getters, and the two counts in `poll` | The C core logs a line for each ("IDR frame request sent", "Invalidate reference frame request sent"), which stream-cadence counts under `--net`; nightjar-core logs neither. The viewer logs both counts when the stream stops (`src/network_core/nightjar.rs`) |
| `nightjar/protocol/client/src/control/driver.rs` | On Unix, `Waker` also wakes the control thread through a local `UnixDatagram` pair, before its UDP datagram to the control socket's own address. The run loop takes the wakes it has seen (`Waker::drain`) and waits on the control socket and the pair together (`ControlSocket::wait_or_woken`). The datagram stays as upstream sends it, so upstream's tests and other platforms see no change. Each change carries a `Korri` comment | The wake datagram crosses the loopback. Under stream-cadence's `--net 'delay 30ms'`, the pair's loopback delays it 30 ms, so a key press waited for the control thread's next `MAX_IDLE_WAIT` tick (0 to 10 ms) and arrived 4 to 6 ms later than the C core's. `tests/nightjar_input_wake.rs` shows it: a median 5.16 ms and a maximum 9.65 ms beyond the delay before the edit; after it, medians of 0.15 and 0.21 ms and maximums of 0.25 and 0.33 ms in two runs |
| `nightjar/protocol/client/src/control/driver.rs`, `nightjar/protocol/client/src/input/queue.rs` | The control thread holds input behind waiting reliable data (`transmit`, up to `INPUT_HOLD_LIMIT`) only while the queue holds no unreliable packet: `InputQueue::holds_unreliable` names a touch, pen, controller touch or touchpad move, or a motion reading, and `transmit` then drains the queue as before. The drop policy stays the C core's: a full queue refuses the new packet, and the viewer logs it (`src/network_core/nightjar.rs`). Each change carries a `Korri` comment | moonlight-common-c's input thread hands each packet to ENet as it comes and waits only for a reliable one to leave, at most 10 ms (`InputStream.c`, `ControlStream.c`); it holds no unreliable packet. nightjar-core held touch moves in its 150-packet queue behind waiting reliable data, so a burst found fewer free slots than the C core's. `tests/nightjar_input_hold.rs` shows it: with reliable data waiting, 60 touch moves stayed queued and a burst of 100 lost 11 to a full queue; after the edit none stayed and none was lost. The Android proof's run 1 of 2026-11-18 lost 2 of 2060 touch moves so (inferred: the proof does not show that the window was full then) |

The workspace root `nightjar/protocol/Cargo.toml` still names members that
are not copied. Cargo reads it only for the members' inherited fields
(`version`, `edition`, `license`, `rust-version`), and the viewer is not in
that workspace, so they are never loaded.

Not used from upstream: `protocol/.cargo/config.toml` applies only to cargo
runs started inside `protocol/`. It sets `--cfg aes_armv8 --cfg
polyval_armv8` for aarch64: RustCrypto's `aes` 0.8 and `polyval` 0.6 compile
their ARMv8 AES and PMULL code only under these cfgs, and nightjar-core
refuses to build for aarch64 without them (`client/src/crypto.rs:10`), so
that no aarch64 build decrypts in software by mistake. They concern those
two crates, not nightjar-core's own code, so no edit here replaces them:
the viewer's Android check and app give rustc the same two cfgs for
arm64-v8a (`../android.nix`). Linux aarch64 would need the same; the
viewer's Linux packages build nightjar-core on x86_64 only.

Code in these trees that other projects wrote, as their source files name
it: nightjar-core ports parts of moonlight-common-c and moonlight-android
(Cameron Gutman and contributors), ClassicOldSong's moonlight-common-c
(Artemis), Moonlight V+ and Moonlight V+ PC (qiin2333), and Nonary's
moonlight-common-c, all GPL-3.0; and its `client/src/fec.rs` is "Ported
from nanors (MIT, Joseph Calderon)". nanors' licence is `nanors-licence.nix`
here, pinned by hash to the nanors commit that created it. Every artefact
that carries nightjar-core carries that text in its notices: the PC
package's `share/licenses/korri-viewer/NOTICES.txt` and its corresponding
source's `licenses/NOTICES.txt` (`../notices.nix`), and the Korri app's
release notices (`../../android-app/notices.nix`).

`check-licence-mentions.sh` reads every line with one of its words, in any
case, of every text file in these trees but `nightjar/LICENSE.txt` and
`rubylight-native-api/LICENSE`. Its words:

- anywhere in the line: "licenc" and "licens" (licence, license, sublicense,
  relicensed, unlicensed, licensor, licensee, LicenseRef-, license_file),
  "copyright" (copyrighted, copyrights), "public domain" (also with "-", "_"
  or nothing between), "©", and "(c)" before a year;
- with no letter, digit or underscore just before: "GPL", "LGPL" and "AGPL"
  ("GPL v2", "GPL 2.0", "GNU GPL version 2", "GPL-3.0", "LGPLv2.1");
- as whole words, also with a version glued on ("Apache2"): MIT, BSD, 0BSD,
  Apache, ISC, Zlib, MPL, CC0, CC-BY, BSL, Boost, EUPL, WTFPL, OFL, EPL,
  CDDL, Artistic, Unicode-DFS, "Unicode-" and a digit, SSLeay, and "(c)".

`licence-mentions.txt` records each such line word for word, with its file
and what covers it: a notices part (`nanors`, or `nightjar-core and the
Rubylight crates` for the GPL-3.0 projects that nightjar-core ports from), a
crate of the crate lists (`crate:rusty_enet`), or `-` when the line names no
licence of the code. Both notices' builds run the script after its tests
(`check-licence-mentions.test.sh`). They fail on a line that is not
recorded, a new one in a listed file too; on a recorded line that is gone;
and on a part or crate that the notices lack. 33 lines are recorded today
(2026-11-18).

The script does not catch:

- a port or a licence on a line with none of the words, such as "Ported
  from foo" alone, or a licence name that is not a word here ("X11", "NCSA",
  "Python-2.0", "PostgreSQL") without "licence" or "copyright";
- a whole word with a letter, digit or underscore just before it, or a
  letter or underscore just after it ("BSDish", "zlib_rs"); "GPL", "LGPL" or
  "AGPL" with a letter, digit or underscore just before it ("nonGPL"); "(c)"
  with a letter, digit or underscore just before it and no year after it
  ("Foo(c) Bar", "repr(C)");
- a phrase or a word split across two lines ("public" and "domain", "licen-"
  and "sed"), and a word in another language or spelled with other
  characters ("Lizenz", "ⓒ", full-width letters);
- binary files, which hold a NUL byte, and the names of files;
- text added to `nightjar/LICENSE.txt` or `rubylight-native-api/LICENSE`:
  the notices' builds check only that each is the GPL version 3;
- files outside `nightjar/` and `rubylight-native-api/`;
- a wrong part or crate: the script checks that it is in the notices, and a
  person decides that it covers the line.

So read the header of each new source file when the pin moves.
Nightjar's own non-GPL parts that the viewer does not copy (its apps'
shaders, fonts and renderers) are not in these trees.

To move the pin: copy the same paths from the new commit with `git archive`
in one commit with no edits, then make the edits above again in the next.
