# Korri's signed Nix cache

Korri's own outputs are published to [korri-os/nix-cache](https://github.com/korri-os/nix-cache)
as a standard Nix binary cache served from GitHub release assets. Devices and
builders substitute from it like any other cache; nothing here is a bespoke
archive format.

```
cache root  https://github.com/korri-os/nix-cache/releases/download/cache/
metadata    release tag `cache`   — mutable: nix-cache-info and signed .narinfo
payloads    release tag `batch-*` — NAR files, one release per batch
```

That address is written once, in `nix/cache/identity.nix`, together with the keys
that may sign for it. `nix/base/default.nix` builds every device's substituter
and trusted-key lists from it, the flake re-exports it as `cache` for the
publisher and for the repositories that configure Korri's builders, and `nix
flake check` warns about the unknown output — a warning worth more than a second
copy of the address.

## What goes in it

Only outputs no public cache already serves. `github-cache.py prepare` takes a
`--upstream-cache` per public cache and drops every path those caches can
verify, so glibc, gcc-lib, avahi and the rest are never uploaded. The first
publication signed ten paths and uploaded four: `korrid`, `korri-inputd`,
`korri-portal`, `korri-kiosk`, 9.0 MB total.

Nobody maintains that list of caches. `upload-staged.sh` reads the substituters
a real device trusts and treats every entry except Korri's own as an upstream to
filter against, which makes the binding rule structural: **the publisher cannot
skip a path because of a cache its consumers do not trust.** If it could, the
device would resolve Korri's output and then fail on a dependency it has no
source for — at install time, not at publish time. Adding a cache to
`nix/base/default.nix` widens the filter in the same change.

`prepare` does not take the list on faith either: it fetches each upstream's
`nix-cache-info` and every candidate `.narinfo`, and verifies the hash before dropping
a path, so a wrong list over-publishes rather than breaking a closure.

This matters more than it sounds. `nix copy` works on closures, so publishing
`inputplumber-korri` without the filter would upload 2198 MiB to deliver
136 MiB of InputPlumber.

## Signing

Each builder signs its own work, so one machine can be revoked without
reissuing the others.

| Builder | Public key |
|---|---|
| fuji (aarch64) | `korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ=` |
| zao (x86_64) | `korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4=` |

`nix/cache/identity.nix` holds that list. Adding a builder means adding its key
there, and revoking one means deleting its line; both devices and builders read
the result, so there is no second list to remember.

Secrets live at `~/.config/korri/cache-key.secret` on each builder, mode 0600,
and are backed up outside this repository. `require-sigs = true` stays on
everywhere; a signature bypass is never the fix for a failed download.

## Publishing

`github-cache.py` is vendored from [korri-os/plugins](https://github.com/korri-os/plugins)
`nix/github-cache.py`, unchanged apart from this note, so both repositories
publish the same format and a consumer needs one importer.

A builder publishes with one command and no arguments:

```sh
nix run .#korri-nix-cache-upload               # or -- --dry-run
```

`post-build-hook.sh` signs every locally built path into a staging file cache
(`/var/cache/korri-nix-cache` by default), which is fast and never fails a
build. `upload-staged.sh` is the batch half: it derives the upstream filter,
prepares a batch, creates that day's `batch-<date>` release if it is missing,
uploads NARs and then metadata, and clears the staged entries it handled. An
idle builder exits 0 without contacting GitHub, so a timer failure always means
a real failure.

The underlying steps stay available for one-off work:

```sh
nix run .#korri-nix-cache -- export  "$WORK/cache" --key-file ~/.config/korri/cache-key.secret PATH...
nix run .#korri-nix-cache -- prepare "$WORK/cache" "$WORK/prepared" \
  --nar-base-url https://github.com/korri-os/nix-cache/releases/download/BATCH/ \
  --upstream-cache https://cache.nixos.org
nix run .#korri-nix-cache -- upload  "$WORK/prepared" --repo korri-os/nix-cache \
  --tag BATCH --cache-tag cache --part nars
nix run .#korri-nix-cache -- upload  "$WORK/prepared" --repo korri-os/nix-cache \
  --tag BATCH --cache-tag cache --part metadata
```

`publish` is the plugins-repo path and requires the NAR tag to resolve to the
source commit. That cannot hold when source and cache live in different
repositories, so core uses `upload` and accepts the weaker provenance: the
signature still binds the bytes to a builder.

## Verification

Proven on 2026-09-11 from zao against an empty chroot store, `require-sigs =
true`, `max-jobs = 0`, `fallback = false`:

- with `korri-cache-fuji-1` trusted, `korrid` and its six upstream dependencies
  were substituted into the empty store;
- with that key removed, Nix refused: *"ignoring substitute … as it's not
  signed by any of the keys in 'trusted-public-keys'"* and the build failed
  rather than falling back.
