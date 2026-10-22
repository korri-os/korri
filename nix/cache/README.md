# Korri's signed Nix cache

Korri's own outputs are published to [korri-os/nix-cache](https://github.com/korri-os/nix-cache)
as a standard Nix binary cache with metadata on GitHub Pages and payloads on
GitHub Releases. Devices and
builders substitute from it like any other cache; nothing here is a bespoke
archive format.

```
cache root  https://korri-os.github.io/nix-cache/
metadata    branch `cache-index` — nix-cache-info and signed .narinfo, via Pages
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

Pages serves narinfos directly, so `prepare` checks Korri's own cache as a
trusted upstream. The old post-prepare self-filter has been removed: upstream
verification checks signed identity, trusted signatures and transport metadata,
not only `StorePath`. Missing narinfos retain their payloads; shared compressed
NARs stay if any retained narinfo needs them.

`publish.sh` also passes the destination from `identity.nix` as `--cache-url`.
Only for that destination, `prepare` keeps an already-published output when
`StorePath` and `References` match but `NarHash` or `NarSize` differs. Its
signature must still verify against a device-trusted key, and transport metadata
must still be well formed. It prints one line naming each non-reproducible path,
omits the local narinfo and any unneeded local NAR, and never changes the
published narinfo. Devices substitute the published bytes; matching references
preserves the closure's dependency graph, but does not prove those bytes behave
like the locally tested build.

Other upstreams, including cache.nixos.org, still require the full signed
identity to match. This exception handles Korri's parallel publications without
weakening checks for unrelated caches. Without `--cache-url`, all upstreams
remain strict. The destination must also appear in `--upstream-cache`; naming
it does not grant device trust.

`prepare` does not take the list on faith either: it fetches each upstream's
`nix-cache-info` and every candidate `.narinfo`, validates metadata and checks
trusted signatures before dropping a path. A missing narinfo keeps the local
output; malformed or untrusted metadata fails preparation.

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
`nix/github-cache.py`, with three intentional Korri patches. The first: after a failed upload
POST, refresh the release and its assets. Accept an already-uploaded asset only
from the same release ID with unchanged draft state and no immutability flag,
using the existing size, SHA-256 digest, and uploaded-state checks. Narinfos may
instead pass the existing equivalence check, which preserves the first URL and
requires all other fields, including signatures, to match. If refresh or
verification fails, preserve the original POST failure. Never retry a write,
clobber, or delete an asset. Both repositories still publish the same format;
this patch changes only concurrent-upload reconciliation, not the cache protocol.

The second moves metadata publication to the `cache-index` Git branch and Pages.
The publisher commits additions together and pushes without force. On a rejected
concurrent push, it fetches the new tip, rechecks every collision, and reapplies
its additions (at most 20 attempts). An existing narinfo must match bytes or pass
the same equivalence rule: only `URL:` may differ, and the first URL stays.
`nix-cache-info` must match exactly. A mismatch fails the whole commit. The
publisher then waits up to 20 minutes for HTTP 200 and the exact retained bytes
of every submitted metadata file at the plain Pages URL. A timeout fails the
publish, even if the Git push succeeded. Retrying publication is safe.

The third keeps trusted non-reproducible outputs already served by Korri's own
cache during preparation, as described above. It does not relax index-commit
collision checks: if a different output first appears after preparation, the
concurrent metadata push still fails rather than replacing it.

The old `cache` release is frozen, not deleted. Publishers never write metadata
to Releases and devices list only Pages, not both roots. Keys and signatures are
unchanged. The vendored copy in `korri-os/plugins` has not been changed.

Name the devices, from any checkout, on any machine that can build them:

```sh
nix run .#korri-nix-cache-publish -- odin2portal rg353m
```

That builds each device's system closure, signs it into a local NAR cache with
this machine's key, drops every path a cache the device trusts already serves,
creates that day's `batch-<date>` release if it is missing, and uploads NARs and
then metadata, waiting for Pages visibility before success. Nothing else on the machine is touched or inspected.

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

`--package` also covers what a closure cannot reach: outputs a device does not
install but a builder still needs. The kernel proved it. Its `dev` output is what
an out-of-tree display driver compiles against, no device installs it, so no
closure carries it — and a cold runner rebuilt an entire kernel for 1 hour
50 minutes to obtain one output it could not download. Each device job therefore
names its kernel beside its closure, and `--package` publishes every output of
what it is given.

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
  --upstream-cache https://cache.nixos.org \
  --upstream-cache https://korri-os.github.io/nix-cache/ \
  --cache-url https://korri-os.github.io/nix-cache/
nix run .#korri-nix-cache -- upload  "$WORK/prepared" --repo korri-os/nix-cache \
  --tag BATCH --index-branch cache-index --cache-url https://korri-os.github.io/nix-cache/ --part nars
nix run .#korri-nix-cache -- upload  "$WORK/prepared" --repo korri-os/nix-cache \
  --tag BATCH --index-branch cache-index --cache-url https://korri-os.github.io/nix-cache/ --part metadata
```

`publish` is the plugins-repo path and requires the NAR tag to resolve to the
source commit. That cannot hold when source and cache live in different
repositories, so core uses `upload` and accepts the weaker provenance: the
signature still binds the bytes to a builder.

## Pages deployment and retention

`pages.yml` is the reviewed source installed as `.github/workflows/pages.yml`
on `korri-os/nix-cache`'s `cache-index` branch. Pages is configured with
`build_type = workflow`, and the `github-pages` environment permits that branch.
Pushes trigger `actions/upload-pages-artifact` and `actions/deploy-pages` with
Pages write and OIDC permissions. The publisher token only needs contents write;
installing or updating the workflow itself also needs workflow permission.
`.nojekyll` ships with the index. The workflow copies metadata without rewriting
it, and excludes repository/workflow files from the site.

A single main push has one cross publisher and seven device publishers, beside
zao and fuji. Two nearby runs can exceed the soft limit of 10 branch-built Pages
builds/hour. [GitHub's documented limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits)
exempt custom Actions deployments from that limit, so we use Actions rather than
branch builds. Workflow concurrency never cancels an active deployment; pending
pushes coalesce. Each deployment checks out the latest index tip so a delayed
run cannot roll metadata back. No Korri CI workflow needs to be dispatched.

The free site still has a 1 GB published-size limit and a soft 100 GB/month
bandwidth limit. Only metadata uses Pages bandwidth. Dated payload releases
still have the 1,000-asset limit, and old payloads must remain available while
narinfos reference them. This change does not implement payload retention or
batch rotation beyond the existing daily tags.

### Measured HTTP caching (2026-10-22)

On the Denver Fastly edge, `nix-cache-info` and narinfos returned HTTP 200 with
`Cache-Control: max-age=600` and an `Expires` value 10 minutes after `Date`.
A missing narinfo returned HTTP 404 without `Cache-Control` or `Expires`, but
was still cached: the second plain-URL request 3 seconds later returned
`X-Cache: HIT`, `Age: 3`, and the same `X-GitHub-Request-Id`. A cached 404's
exact lifetime was not measured. A probe of the package narinfo before publish
returned 404; the publish later passed the exact-byte visibility wait without
cache-busting query strings. These are observations at one edge, not a global
CDN guarantee. The bounded 20-minute wait allows deployment and cache expiry;
an outage or longer delay still fails rather than falsely reporting success.

## One-off migration (2026-10-22)

All 988 assets from release `cache` were downloaded: 987 signed narinfos and
`nix-cache-info`, 1,320,148 bytes total. Each local size and SHA-256 digest matched
the GitHub asset inventory before seeding. The publisher's index-commit code
seeded them without changing bytes in index commit
`777a1193014e2126fe3b97bdefae1d4bb6af2cd0`. The old release, its assets, and its tag
stay untouched. Pages URL from the API: `https://korri-os.github.io/nix-cache/`.

A running old generation only knows the frozen release URL. For its first
fetch of a new generation, run as root (using the exact prebuilt path):

```sh
nix build --no-link /nix/store/EXACT-NEW-SYSTEM-PATH \
  --option extra-substituters https://korri-os.github.io/nix-cache/
```

Keep signatures required, builds disabled, and the existing keys. Activate only
after the full closure is present. The new generation configures Pages itself;
there is no runtime fallback or dual listing. No device was changed during this
migration. See [device-cache policy](../device-cache/README.md).

### Live proof (2026-10-22)

All 988 seeded metadata files returned their exact original bytes from Pages.
`curl` also checked `nix-cache-info` and these signer samples:

| Signer | Narinfo |
|---|---|
| fuji | `w5bkbzzb4rpxkk4gdmjlryb6464806yr.narinfo` |
| zao | `2v1297b520yynwkwxchay0hd4fqz64ky.narinfo` |
| CI | `0059d2hwfis7k6wl0sk1r0l1sr9jzg1g.narinfo` |

The fuji narinfo's absolute `URL:` points to
`https://github.com/korri-os/nix-cache/releases/download/batch-2026-09-11/13208kfzmqwl2rb87sx9qlmlq8yms2ymcdh15dnlp7ksfrlk336a.nar.xz`.
Nix fetched its `korrid-0.0.0` path and six public dependencies into the empty
chroot store `/tmp/korri-cache-pages-cold-store-20261022`: 15.4 MiB downloaded,
77.4 MiB unpacked, no build. Settings were `require-sigs = true`, `max-jobs = 0`,
`fallback = false`, no builders, substituters Pages plus cache.nixos.org, and
the identity keys plus NixOS's stock key. Nix reported copying korrid from Pages;
the narinfo directs the payload download to Releases.

The real `--package packages.x86_64-linux.rg353m-inputplumber-data` command on
zao published one 2,162-byte NAR and signed narinfo
`3nhx3h6v488lqdbisjxvdqwqj08fnp6f.narinfo`. Its signature is
`korri-cache-zao-1`, its URL points to `batch-2026-10-22`, and index commit
`b92b9dadd0163e227f7c7c59667b2c891bffa502` holds the new metadata. The publisher waited
for exact Pages bytes before success. Repeating the command verified Pages as
a normal signed upstream and reported all one path already served, with no
upload. No device was contacted and Korri's CI workflow was not dispatched.

## Verification

Run the offline publisher regression checks without a device build or upload:

```sh
nix run .#korri-nix-cache-check
```

`pages-cache.test.py` races two publishers against a local bare Git remote,
forcing a rejected push and retry. It checks retained equivalent URLs, refused
signed-field changes, cache-info replacement, symlinks, unrelated push failures,
and exact-byte visibility with a local HTTP server,
including stale responses, HTTP errors, and timeout failures.

The prepare checks use a local HTTPS server, real signed file caches, and
shared compressed NARs. They cover
partial and complete filtering, missing metadata, refusal of mismatched signed
identity or untrusted signatures (including content-addressed records), preserved
signatures, and downloads into an empty local store with builds disabled. They
also verify that a trusted non-reproducible output at the destination keeps the
published narinfo, uploads no orphan local NAR, and substitutes the published
bytes into an empty store. Changed references, untrusted signatures, malformed
transport metadata and NAR differences at other upstreams still fail.
Two concurrent uploader processes also exercise a file-backed `gh api`
release-assets contract. A barrier makes both clients read the initial asset
snapshot before either POSTs. The checks cover duplicate NARs, equivalent
narinfos, mismatched bytes/signatures, unfinished assets, changed releases, and
failed refreshes without contacting GitHub.

Proven on 2026-09-11 from zao against an empty chroot store, `require-sigs =
true`, `max-jobs = 0`, `fallback = false`:

- with `korri-cache-fuji-1` trusted, `korrid` and its six upstream dependencies
  were substituted into the empty store;
- with that key removed, Nix refused: *"ignoring substitute … as it's not
  signed by any of the keys in 'trusted-public-keys'"* and the build failed
  rather than falling back.
