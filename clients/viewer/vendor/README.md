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
| `nightjar/protocol/client/src/control/session.rs` | `ControlSession` counts the IDR requests and the reference frame invalidations that `poll` queues for the host (`idr_requests_sent`, `rfi_requests_sent`). Each change is marked `Korri` | The C core logs a line for each ("IDR frame request sent", "Invalidate reference frame request sent"), which stream-cadence counts under `--net`; nightjar-core logs neither. The viewer logs both counts when the stream stops (`src/network_core/nightjar.rs`) |

The workspace root `nightjar/protocol/Cargo.toml` still names members that
are not copied. Cargo reads it only for the members' inherited fields
(`version`, `edition`, `license`, `rust-version`), and the viewer is not in
that workspace, so they are never loaded.

Not used from upstream: `protocol/.cargo/config.toml` applies only to cargo
runs started inside `protocol/`. It sets `--cfg aes_armv8 --cfg
polyval_armv8` for aarch64, and nightjar-core refuses to build for aarch64
without them (`client/src/crypto.rs`). The viewer's aarch64 build with the
feature therefore needs those flags; it has not been tried.

To move the pin: copy the same paths from the new commit with `git archive`
in one commit with no edits, then make the edits above again in the next.
