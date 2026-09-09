# Plugin release preparation

`.github/workflows/plugin-repository.yml` prepares `@korri:tailscale` on standard
`ubuntu-24.04` and `ubuntu-24.04-arm` runners. It has not been run or published as
part of this change. No official catalog URL is configured by the workflow.

## Owner controls

Only the repository owner's manual dispatch from `main` can pass the guard.
A rerun must also come from the owner. There are no PR, push, or tag triggers.
Actions are pinned to the existing workflows' commit hashes. Build, Rust,
package, module, publisher and x86_64 VM checks run with `contents: read`.
A missing architecture or failed check blocks assembly and draft creation.
Core binaries without `plugin.ts` are not catalog entries. Kernels, images,
Android builds, and general core binary caching are outside this workflow.

Supply an explicit GitHub `OWNER/REPOSITORY`, release tag, and new plugin release
label. The label does not come from Tailscale's upstream binary version. With
`create_draft` false, the workflow only produces seven-day Actions artifacts:

| Artifact | Content and use |
|---|---|
| `plugin-x86_64-linux`, `plugin-aarch64-linux` | Complete file-cache tar and its producer-emitted `record.json`. These are intermediate build artifacts, not a live repository. |
| `plugin-release-assets` | Both verified tar files, intended for GitHub Releases. Each must be strictly smaller than 2 GiB. |
| `plugin-catalog` | Only `catalog.json`, intended for a separately approved HTTPS catalog host. Never large archives or a Nix substituter. |

The Rust publisher owns the archive filename: `package::unit_name(id)`, explicit
release label, and Nix system. `archive-name` loads the actual package declaration.
The build uses that name in both the intended archive URL and the upload.
`catalog` merges producer records with the same `Catalog` serialization and
reader validation, including duplicate and size limits. `verify-release` checks
the complete expected platform set, release, URLs, regular files, sizes and
hashes before external writes. No YAML or jq constructs catalog records.
The existing archive producer verifies the complete content-addressed Nix cache
before packing. Devices retain their own independent checks.

## Draft writes are separate from publication

`create_draft: true` explicitly approves only draft creation and archive upload.
The separate write job uses the `plugin-release` environment. Before its first
use, the owner must review that environment's approval and branch protections.
These settings have not been inspected or changed here. The job's event, actor,
rerun-actor, branch and explicit-approval guards do not depend on the environment
being configured correctly.

Draft writes are limited to the current repository because its `GITHUB_TOKEN`
does not authorize another repository. Preparation may target another explicit
repository, but uploading there needs a separately reviewed operational path.
No cross-repository secret or account permission is introduced.

The destination tag must already exist and resolve to this run's exact source
commit. The workflow neither creates nor moves tags. Before creation it lists
**all pages** of releases and rejects any draft or published release with that
tag. Failed reads, including authentication and network errors, stop preparation.
`gh release create --draft` does not enforce this uniqueness by itself.

The documented REST create-release request sends `draft: true` and captures the
new release ID and upload URL. Every later read and upload uses that exact ID;
downloads use the exact IDs returned for uploaded assets, never another lookup
by tag. Response IDs and URLs must match the requested repository and the fixed
`api.github.com` / `uploads.github.com` HTTPS origins before use. Assets are
POSTed without replacement or deletion. Preparation checks tag uniqueness and
draft state before and after each upload, compares the stored asset IDs, and
downloads the stored bytes for hash verification. It rechecks asset identity and
draft state after verification. A failed upload leaves a partial draft for owner
inspection; retries reject it rather than silently repairing or overwriting it.
The write step uses a validator built before the write token is exposed.

**Do not publish or edit the draft, change its tag, or start another release for
that tag while preparation runs.** GitHub supplies no transaction across list,
create, upload and verification requests. Workflow concurrency serializes these
workflow runs, not other owners or API clients. A concurrent owner can change
state between a check and a write; later detection cannot undo that write. Exact
IDs prevent retargeting an upload to another release, but cannot freeze the draft
or make tag uniqueness atomic. The final check is a point-in-time observation,
not proof that another owner cannot publish immediately afterward. Repeated
paginated checks add API requests; a rate limit can leave preparation incomplete.

## Immutability and the final operator gate

GitHub's documentation, read on 2026-09-08, identifies the real control:
repository **Settings → Releases → Enable release immutability**. Organization
policy may also enforce it. This applies only to future releases. The documented
create-release request does not provide an `immutable` input; a response field
is not a request setting. We do not send an undocumented field or infer account
settings from public documentation.

Sources:
- [Preventing changes to your releases](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes).
- [Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases).
- [Create a release](https://docs.github.com/en/rest/releases/releases#create-a-release).
- [List releases](https://docs.github.com/en/rest/releases/releases#list-releases) and [get a release by ID](https://docs.github.com/en/rest/releases/releases#get-a-release).
- [Upload a release asset](https://docs.github.com/en/rest/releases/assets#upload-a-release-asset), [list assets](https://docs.github.com/en/rest/releases/assets#list-release-assets), and [download by asset ID](https://docs.github.com/en/rest/releases/assets#get-a-release-asset).
- [`gh api`](https://cli.github.com/manual/gh_api).
- [Standard GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

Actual repository settings and free runner entitlement have not been verified.
Use standard runners only; do not enable larger runners or paid services to
resolve a limit. This workflow intentionally has **no publish operation**.
The owner must separately approve and perform the remaining steps:

1. Confirm repository visibility/free runner availability, tag identity,
   environment approval rules, and release immutability before publication.
2. Inspect the complete draft and both downloaded archives. Publish the draft
   only after the immutability control is enabled. Confirm its **Immutable**
   indicator and verify the public downloads. If the control is unavailable,
   leave the draft unpublished; do not claim immutable delivery.
3. Choose the real HTTPS catalog destination. Deploy the small validated
   `catalog.json` only after every referenced Release asset is public. Keep
   archives in Releases, not Git history or Pages. A later catalog update must
   retain any still-offered records deliberately; this workflow produces one
   explicitly selected plugin release, not a cumulative repository history.
4. Configure that real URL through `officialCatalogUrl` after owner approval,
   then perform actual GitHub download and device acceptance separately.

The cost is full closure conversion/downloads per architecture and repeated
build work on uncached runners. Standard free runner disk, time or quota limits
may still stop preparation. Actions artifact retention is not release hosting.
General core packages now have no Korri-hosted substitute service; Releases is
not a drop-in Nix cache. Device misses must fail without local or remote builds.

## Local checks

Run on a build machine with a writable Nix store:

```sh
nix run .#korri-publisher-check
nix build .#checks.x86_64-linux.korri-publication-workflow --no-link
```

The opt-in app supplies the dynamically built `korri-tailscale`, locked Nix, and
current Nix system. It runs the actual ignored publisher tests explicitly;
missing configuration fails. CA conversion is not run inside a pure derivation.
The ordinary Rust/HTTPS and publication CLI tests remain sandboxed in the
`korri-plugin-host` package check. The workflow check parses YAML, runs offline
guard tests, controlled-process REST orchestration tests, and actionlint. The
process tests exercise pagination, read failures, concurrent tag ambiguity,
exact-ID uploads and downloads, draft-state changes, invalid response URLs,
byte verification, and failed-draft retry refusal. They use local files and
configured CLI processes; they make no network requests or GitHub writes.
