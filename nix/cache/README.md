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

What a device installs, minus what it can already get elsewhere.

The unit of publication is a device's system closure, because that is the unit a
device installs and it may not build any of it: `nix/device-cache/nixos-module.nix`
forces `max-jobs = 0` and `fallback = false`. Publishing only Korri's own packages
would not be enough — a closure also holds hundreds of small NixOS-generated
paths, unit files and `/etc` fragments among them, that no public cache serves
and that no naming scheme identifies as Korri's.

`publish.sh` asks the device being published which caches it trusts, and skips any
path one of them can already serve. So two kinds of path are dropped: those a
public cache serves, and those Korri has already published. What remains is new.
Nobody maintains a list, which makes the binding rule structural: **the publisher
cannot skip a path because of a cache its consumers do not trust.** If it could,
the device would resolve Korri's output and then fail on a dependency it has no
source for — at install time, not at publish time. Adding a cache to
`nix/base/default.nix` widens the filter in the same change.

Korri's own cache is the one entry `prepare` cannot check. A GitHub release
download answers with a redirect to a signed URL carrying a query string, and
`github-cache.py` refuses to follow a redirect like that for a cache location — a
reasonable rule for a tool that treats cache URLs as trusted input. So `publish.sh`
hands `prepare` the caches it can read, and applies Korri's own cache to
`prepare`'s output itself, comparing `StorePath` in the published narinfo before
dropping a path.

`prepare` does not take the list on faith either: it fetches each upstream's
`nix-cache-info` and every candidate `.narinfo`, and verifies the hash before dropping
a path, so a wrong list over-publishes rather than breaking a closure.

This matters more than it sounds. `nix copy` works on closures, so publishing
`inputplumber-korri` without the filter would upload 2198 MiB to deliver
136 MiB of InputPlumber. The first publication signed ten paths and uploaded
four — `korrid`, `korri-inputd`, `korri-portal`, `korri-kiosk`, 9.0 MB total.

Publishing deliberately does **not** happen through a machine-wide Nix
`post-build-hook`. A hook is handed store paths with no idea which project asked
for them, so on a machine that builds anything else it would publish that too —
including the builder's own system closure. The closure Korri cares about is known
when Korri builds it, so that is where publishing happens.

## Signing

Each builder signs its own work, so one machine can be revoked without
reissuing the others.

| Builder | Public key |
|---|---|
| fuji (aarch64) | `korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ=` |
| zao (x86_64) | `korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4=` |
| CI | `korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c=` |

CI's key is the weakest of the three. It lives in GitHub Actions secrets, so
anyone who can land a workflow change can sign with it, and it is trusted because
a device that cannot install what CI built gains nothing from CI. Revoking it is
the same one-line deletion as revoking a machine.

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

Name the devices, from any checkout, on any machine that can build them:

```sh
nix run .#korri-nix-cache-publish -- odin2portal rg353m
```

That builds each device's system closure, signs it into a local NAR cache with
this machine's key, drops every path a cache the device trusts already serves,
creates that day's `batch-<date>` release if it is missing, and uploads NARs and
then metadata. Nothing else on the machine is touched or inspected.

Two environment variables change how it runs:

| Variable | Effect |
|---|---|
| `KORRI_CACHE_SECRET_KEY` | This builder's signing key. Defaults to `~/.config/korri/cache-key.secret`, and must be private and outside the store. |
| `KORRI_CACHE_STORE` | Keep the local NAR cache at this path instead of a temporary directory, so a later publish does not compress the same paths again. |

Compression is zstd, not Nix's default xz. Most of a device closure is public and
is dropped moments after it is compressed, so the work is thrown away either way
and the fast setting is the honest one.

### From CI

`.github/workflows/nix-cache.yml` runs the same command on every push to main, on
GitHub-hosted runners, so publishing never waits for a machine of ours to be
awake. It has no `pull_request` trigger: the workflow holds a signing key, and a
fork must not reach it.

It takes two stages because a device closure is not one architecture. The
odin2portal closure holds 544 x86_64 derivations beside 6460 aarch64 ones — the
kernel and firmware are cross-built, by design, because the aarch64 builder cannot
spare the disk. The x86_64 job publishes those outputs first with `--package`, and
the aarch64 jobs then substitute them rather than failing on work their runner
cannot do.

The second stage is one job per device. Signing a closure writes a second,
compressed copy of it beside the closure, so three devices on one runner need
three of each; the first attempt built all three and then ran out of disk while
signing, four hours in. Per-device jobs bound the peak to one closure, run the
three at once, and keep one device's failure from hiding another's result.

The cache pays for itself immediately and measurably: the cross job took 32
minutes when it built both kernels and **2 minutes 14 seconds** on the next run,
when it substituted them from what it had just published.

That is also why `--package` exists. A device closure is the unit that matters,
but a machine can only publish the part of it that its own system can build.

CI needs two secrets on `korri-os/korri`: `KORRI_CACHE_SECRET_KEY`, the CI signing
key, and `KORRI_CACHE_GITHUB_TOKEN`, a token with contents write access to
`korri-os/nix-cache` and nothing else. The automatic `GITHUB_TOKEN` cannot be used
— it is scoped to this repository, and the cache lives in another one, which is
part of why the cache has its own repository.

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
