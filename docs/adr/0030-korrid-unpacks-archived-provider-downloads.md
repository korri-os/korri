---
status: proposed
---

# korrid unpacks archives in its core

**Owner answers, 2026-10-23.**

- Q1a: "I think we need to fully support decompression in the core so that
  all plugins can benefit from this." Unpacking is a korrid core capability,
  not a feature of one provider plugin.
- Q1b: zip, 7z and rar, all three.
- Q1e: add an `Unpacking` job state.
- Q1c and the folder question: "we're going to have to come up with or
  research an algorithm for this because there's really no deterministic way
  to truly say where a particular file is. And then it gets a little bit
  tricky when we're talking about zip files of entire applications. For
  example, itch.io will often have zips with tens, hundreds of files inside."
- Q1d: "This feels a little bit arbitrary. Games can be very big. You have to
  remember that this is not just for ROMs. These could be full ISOs, these
  could be full game files with thousands of files inside."

**Coordinator decisions, 2026-10-23.** The owner asked "Does it matter?" for
Q3a and did not have the context for Q2a to Q2d. These are implementation
details, so the coordinator decided them. Each has one line of reason in
"Change 2" and "Change 3".

- Q2a: `body` is an object of string values. korrid form-encodes it and sets
  `application/x-www-form-urlencoded`. POST is the only method besides the GET
  default.
- Q2b: browser rules. 301, 302 and 303 become a GET with no body. 307 and 308
  send the POST again.
- Q2c: no separate declaration. The approved `hosts` stay the boundary.
- Q2d: the existing `SafeText` bound, 8192 characters.
- Q3a: N1. `FinalDownload.filename`, then the URL's last segment.

The remaining owner questions are at the end, under "Owner questions".

**The proposal.** korrid unpacks the whole archive into a work folder in its
cache. It refuses unsafe entries before it writes anything. Then it identifies
what is inside with the rules discovery already has: the system's content
rules and header rules, and ADR 0005's multi-file identities. korrid invents
no new rule to find "the game file". The archive is never a release. The
release is what discovery would find if the same files were in a source, so a
download and a copy in a source join one game. For an application archive,
discovery cannot know the entry point. There the plugin's declaration names
the main file, as ADR 0013 already does. Limits come from the device (free
space, available memory) and from discovery's existing traversal budget, not
from new fixed size caps.

## Changes on main since the first draft

- `cdc330776` ("identify acquired releases by content hash") fixed the old Q4.
  The acquire job now uses discovery's ADR 0005 identity, header excluded
  (`acquisition/game.rs` `publish_and_record`, `identify::identity_of`). The
  old Q4 is removed. This ADR extends that call from one file to a folder.
- `f7d99a83c` lowered the provider response body bound to 512 KiB. It applies
  to provider turns (`fetch_text`) only. The game download streams to disk and
  is not affected.
- `9e217e696` and `ad3da260f` upgraded rquickjs to 0.14 and removed runtime
  code generation from the plugin sandbox. Unpacking runs in Rust, outside the
  plugin sandbox, so nothing here changes.

## Problem

[ADR 0016](0016-korrid-acquires-games-from-provider-plugins.md) lets korrid
acquire a game from a provider plugin. Today korrid refuses every game from a
ROM-site provider. Three rules cause this:

1. **An archive is not a game file.** A system cannot declare `zip`, `7z`,
   `rar`, `gz` or `tar` (`services/korrid/src/plugin.rs` `validate_content`,
   line 1563, from ADR 0005 "Archives are not supported in the first version"
   and the discovery v2 brief). The acquire job checks the name against the
   claim system's extensions before it downloads (`acquisition/game.rs`
   lines 603–616). A `.zip` fails as `NotSystemFile` with no download
   (`acquisition/provider_tests.rs` lines 774–777).
2. **The game download is a GET.** `DownloadEngine::send` builds every hop
   with `client.get` (`acquisition/download.rs` line 174). WoWROMs returns the
   game only to a form POST.
3. **The file name is the URL's last segment** (`policy::basename`,
   `game.rs` line 603, ADR 0016 owner answer Q1). Three of the four sites put
   a token or nothing there.

The owner's answers widen the problem. Games are not only one ROM. They can be
disc images with their cue sheets, multi-disc sets, ISOs of several GiB, and
application folders with hundreds or thousands of files. A model that keeps
"one content file" from an archive does not fit.

## Evidence

Live evidence, gathered 2026-10-23 by other agents. Sources:
`/tmp/korri-provider-probe/REPORT.md` and its `results/*.json`,
`/tmp/korri-wowroms/README.md`, its `fixtures/` and `plugin/`. The probes read
at most 4 KiB of each game file.

| Site | Formats seen (magic bytes) | Last segment of `FinalDownload.url` | Plugin `filename` | `Content-Disposition` | Method |
|---|---|---|---|---|---|
| coolrom.com (moved from coolrom.com.au) | `.zip` x2 (`504b0304`), `.7z` x1 (`377abcaf271c`) | empty: the URL ends in `/` after a signed token and an expiry time | correct (`Sonic the Hedgehog.zip`) | correct | GET, link expires after about 300 s |
| romhustler.org | `.7z` (`377abcaf271c`), `.rar` (`526172211a07`) | empty: the URL ends in `/` after a token | wrong: the plugin says `.zip` for both | the only correct name | GET |
| retrostic.com | `.zip` | `<name>.zip` | correct | absent | GET |
| wowroms.com | `.zip`, first entry is the content file (`Super Mario World (USA).sfc`) | opaque token (`7bf4e0908e024c5`) | form field `file` (`Super Mario World (USA).zip`) | not recorded | POST `application/x-www-form-urlencoded`, body `emuid`, `id`, `file` |

Other facts from the same evidence:

- No probe checked how many files each archive holds.
- WoWROMs: a GET of the file URL returns a page of PHP notices. The POST needs
  no cookie, Referer or User-Agent. The link step before it works as a GET.
  The file host comes from a pool of five: `dfr.wowroms.com`,
  `filesdw.wowroms.com`, `slave7t.wowroms.com`, `trt.wowroms.com`,
  `wowroms-dw.com` (55 samples).
- retrostic: 2 of 5 sampled files are broken. One is a 172-byte nginx
  "400 Bad Request" page sent as `200 application/zip`. Its first bytes are
  `<html>`, so `html_lead` (`download.rs` line 510) refuses it today. This is
  verified by reading the code, not by a test.
- coolrom: search returns 0 results for SNES and GBA. The site no longer
  lists Nintendo games.
- romhustler: 2 of 5 sampled games resolve. The other 3 answer "Sorry, this
  game is restricted." and become `NonFinalDownload`.
- The two PSX samples are disc games: coolrom `.7z`, 460,762,350 bytes, and
  romhustler `.rar`, 543,960,550 bytes. With this ADR they are in scope.
- The legacy parking-lot item `01KXC8EAAD647X5PBTYMP06T6E` (2026-07-12) said
  romhustler guest downloads were account-gated, coolrom needed a cookie jar,
  and WoWROMs needed a POST after a JavaScript countdown. The 2026-10-23
  evidence replaces the first two: 2 of 5 romhustler games download as a
  guest, and coolrom sent no `Set-Cookie`. It confirms the POST.

## Who uses core unpacking

The owner wants all plugins to benefit. This table says what each existing
consumer gets.

| Consumer | Today | With this ADR |
|---|---|---|
| ADR 0016 provider downloads (ROM sites, PICO-8 BBS) | An archive is refused before the download. | korrid unpacks zip, 7z and rar, then identifies the content. This is the first consumer. |
| ADR 0013 artifact-backed games | The plugin ships its own unpack program. Mega Man Rock N Roll ships `unpack.py` (`aa3840e8f`, now in `simonwjackson/korri-plugins`), which extracts one ZIP. | korrid can unpack a pinned zip, 7z or rar artifact. How a declaration asks for it is owner question 4. The plugin still needs its own program for executable bits (see "Application archives"). |
| ADR 0012 runtime artifacts (FEX, Proton) | `fex-unpack` reads a squashfs and `.pkg.tar.zst` overlays. `cachyos-unpack` reads a `.tar.xz` and edits `toolmanifest.vdf` (`tests/fixtures/first-party/fex/plugin.ts`, `proton/plugin.ts`). | No change. Their formats are not zip, 7z or rar, and Proton needs a file edit. They keep their programs. |
| ADR 0016 package files (Starter pack) | Files arrive unpacked in the plugin package. | No change. |
| Discovery of a person's sources | An archive in a source is not a game. | No change. korrid never writes into a person's source (ADR 0013), and identifying an archive in place would need a copy in the cache. Out of scope. |

## Proposal

### Change 1: korrid unpacks the whole archive

A store job does these steps:

1. It downloads as today: declared hosts, the SSRF and redirect policy, one
   attempt, and the web page and empty-body refusal. This ADR proposes to
   replace the 2 GiB cap with the free-space rule in "Limits". Owner
   question 2 decides.
2. It decides from the first bytes whether the download is an archive, and
   which format. The legacy `ARTIFACT_MAGIC` rows come back: zip is
   `50 4b 03 04`, `50 4b 05 06` or `50 4b 07 08`, 7z is `37 7a bc af 27 1c`,
   rar is `52 61 72 21 1a 07`. The name alone is not enough: romhustler sends
   `.7z` and `.rar` bytes under a `.zip` name.
3. The job state becomes `Unpacking` (owner answer Q1e).
4. korrid reads the whole entry list and checks every entry before it writes
   anything (see "Safe extraction"). One bad entry refuses the whole archive.
5. korrid writes every entry into a `.tmp-*` work folder in the provider
   cache. It counts the bytes it writes and checks free space while it writes.
6. korrid identifies the work folder (Change 1b). The result must be exactly
   one release.
7. korrid deletes the archive, renames the work folder to the ready folder,
   and writes the release with the identified identity.

A download whose bytes are not an archive takes the same path with one file.
korrid moves the file into the work folder under its name (Change 3) and runs
the same identification. Plain downloads and archives then share one rule.

Before the download, korrid accepts a name that ends in a system extension or
in `.zip`, `.7z` or `.rar`. It refuses any other name before the download, as
today. After the download, the bytes decide. A name that says archive with
bytes in no archive format fails as `NotSystemFile`.

Nested archives, split and multi-volume archives (`.7z.001`, `.part1.rar`,
`.rev` recovery sets) and encrypted archives fail with the existing `Unpack`
copy.

### Change 1b: korrid identifies the content with discovery's rules

**What exists.** `discovery::identify::identify_with_rules`
(`discovery/identify/mod.rs`) is the "core seam for registry-independent
consumers and tests". It takes any folder as the root of a `SourceInput`, a
list of `ContentRules`, a `TraversalBudget` and a `HashCache`. It walks the
folder, applies extensions, layouts and header rules, lets `.m3u` own its
discs and `.cue` own its tracks, reads CHD headers, and returns `FoundUnit`s
with a `ContentIdentity`. Its unit tests already run it over plain temporary
folders (`identify/tests.rs`, for example
`cue_tracks_form_one_ordered_identity_and_are_not_separate_units`). A ready
folder is a plain folder. So korrid can run the same function over the work
folder with the claim system's rules, `group = false`, no assignments and no
declared-hash lookups. This is verified by reading the code and its tests. No
test runs it over an acquisition work folder yet.

**What identity results.** Each row follows from `identify/disc.rs` and
`identify/mod.rs` as they are on main.

| The archive holds | Units found | Release identity | Result |
|---|---|---|---|
| One ROM, maybe with a readme or `.nfo` | 1 | `Hash`: SHA-256 and SHA-1 of the ROM, header excluded | Accepted. Files with no system extension are ignored. |
| A disc image with its cue (`Game.cue`, `Game (Track 1).bin`, ...) and a system that declares `cue` | 1 | `Parts`: SHA-256 over the ordered track hashes. The cue is not hashed. | Accepted. |
| A multi-disc set with an `.m3u` | 1 | `Parts`: over the disc identities in playlist order | Accepted. |
| A multi-disc set with no `.m3u`, and a system layout with `{disc}` | 1 | `Parts`: discs combined by folder, title, version and region | Accepted. |
| A multi-disc set with no `.m3u` and no `{disc}` layout | one per disc | | Refused (see below). |
| One ISO (`psp` declares `iso`) | 1 | `Hash` of the whole ISO | Accepted. korrid reads the ISO once more to hash it. |
| One CHD | 1 | `Chd`: the header's raw SHA-1, `sha1:<40 hex>` | Refused for now (see below). |
| Several games | 2 or more | | Refused. |
| An archive inside the archive, or no file with a system extension | 0 | | Refused as `NotSystemFile`. |

A copy of the same files in a paired source gets the same identity, because
discovery computes it with the same function. So the store's release and the
discovered copy join one game, as ADR 0005 requires. This is inferred from the
code, not tested end to end.

**Deltas this needs.**

- **One release per job.** A store job writes one game, one release and one
  provider link (ADR 0016 owner answer Q2). Zero units or several units fail
  as `NotSystemFile`. Cost: an archive that bundles several regions of one
  game, or discs that the system's layouts cannot combine, fails. The person
  can still put such files into a source.
- **Layouts.** A system's layouts describe a person's folder, not an archive.
  If none of them matches an entry path, discovery reports `Unmatched`.
  Proposal: for a work folder, korrid tries the system's layouts and then the
  default layout `**/{title}.{ext}` (SCRIPTING.md, "Content, groups and
  declared games"). The title capture is not used. The claim's title names
  the game (ADR 0016 owner answer Q2). Cost: one more rule that differs from
  a source scan.
- **The release record.** `AcquiredGame` carries `sha256_hex` today, and
  `record_acquired` writes `LibraryIdentity::Hash`. It carries the
  `ContentIdentity` instead and writes `Hash` or `Parts`. Both kinds already
  exist in `config/library.rs` `LibraryIdentity`. No new schema.
- **The ready folder name.** ADR 0016 Q1 names it by the SHA-256 hex. A `Hash`
  and a `Parts` identity both have a `sha256:` value, so the grammar holds. A
  `Chd` identity is `sha1:<40 hex>` and does not fit. No ROM-site sample was
  a CHD. A CHD unit fails as `NotSystemFile` until a real case decides the
  folder name. This is unresolved.
- **The route.** `placement::ready_game_file` accepts a ready folder with
  exactly one regular file. That stays true for one ROM. For a folder with
  several files, the route must find the unit's main file without hashing on
  every launch. Proposal: the route applies discovery's owner order to the
  files with a system extension anywhere in the folder tree. That is one
  `.m3u`, else one `.cue`, else one file. This matches the primary path that discovery gives a unit
  (`FoundUnit.path`). No location is saved, as ADR 0016 owner answer Q3 says.
  Cost: the route reads the folder listing on each launch.

**Plain downloads.** The staging file is named `.acquire-<hex>`. Rust's
`Path::extension` returns nothing for that name, so `identity_of` hashes every
plain download as ordinary bytes. A plain `.chd` download therefore gets a
whole-file SHA-256 today, while discovery gives the same file a `Chd`
identity, so the two never join. Identifying the file under its real name in
the work folder removes this difference. A plain `.chd` download then fails,
like a CHD in an archive, until the folder name for a `Chd` identity is
decided. It also makes a lone `.cue` fail, which is correct: its tracks are
missing.

### Application archives

Discovery cannot know which file in an itch.io-style folder starts the game.
Main already has three answers, and all three put the choice in the plugin's
declaration:

- ADR 0013 artifact-backed games: `file: { artifact, path }` names "the
  release's content file" inside the unpacked folder. The release hashes are
  "hashes of its main content file" (Mega Man Rock N Roll: `data.pck`, not the
  ZIP). korrid passes that path as `contentPath`.
- ADR 0016 package games: `file: { packageFile, path }`, with content-file
  hashes proven by the package build.
- Runners: a native runner's `releases` lists accepted content hashes
  (SCRIPTING.md, "Native runners for specific content"). The runner decides
  how to start the folder. The Rock N Roll runner gets `data.pck` as
  `contentPath` and starts the program beside it.

Legacy agrees. The PortMaster installer unpacked the whole zip and then picked
launch scripts by a plugin rule, the `.sh` suffix
(`legacy:product/plugins/portmaster/src/installer.ts` `isLaunchScript`). No
legacy core code chose an entry point.

So the folder question has two answers. For ROMs and disc images, the rule is
general: discovery's system rules, which plugins declare per system. For
application folders, the rule is plugin-specific: the declaration names the
main file.

- **Declared games (ADR 0013).** No new schema. If a pinned artifact is an
  archive that korrid unpacks (owner question 4), `file.path` already names
  the main file in the unpacked folder.
- **Provider downloads.** A `FinalDownload` is a runtime result, not a
  declaration. No existing producer, consumer, treaty or legacy record names a
  file inside a provider's archive. Legacy `ArtifactAcquisitionHint.file.name`
  names the downloaded file, not an entry in it. The shape is unresolved.
  ADR 0016 also excluded itch.io ("skip itchio"). Owner question 5 asks which
  real case must define it.

**Executable bits.** korrid writes every file as `0644` and every folder as
`0755`, the default of Rock N Roll's `unpack.py`. It never copies an
executable bit from an archive. NVD CVE-2023-49102 (NZBGet) is a remote code
execution that came from an unpacker that kept executable bits. Rock N Roll
also shows that an archive cannot be trusted to carry them: its ZIP "has
MS-DOS attributes and carries no Unix modes", so its executable bit comes from
the plugin's `contents` list. A plugin whose folder needs executable files
keeps its own ADR 0012 unpack program until a declaration for modes exists.
That declaration is unresolved.

### Limits from the device

The owner called fixed size and entry caps arbitrary. Each check below comes
from the device or from an existing bound.

**Safe extraction.** These rules come from Rock N Roll's `unpack.py`, legacy
`safeZipPath` (`legacy:product/platform/archive/zip.ts`) and the staging file
today (`O_NOFOLLOW` in `download.rs`). korrid refuses the whole archive before
it writes if one entry:

- is a symbolic link, a hard link, a device, a FIFO or a socket.
- is encrypted.
- has an absolute path, a drive letter, a `..`, `.` or empty segment, a
  backslash, a NUL or another control character.
- repeats a path that another entry already has.

Each path segment must pass legacy `SafeFileName` (`claims::safe_file_name`)
and the control-character rule of `policy::basename`. korrid creates every
file with `O_CREAT | O_EXCL | O_NOFOLLOW` in a work folder that it made, so a
name that collides on a case-insensitive file system fails instead of
overwriting. After the write, korrid removes write permissions, syncs the tree
and renames it, as `placement::freeze_and_sync` does today.

**Bytes written, not declared sizes.** korrid counts the bytes that each
decoder gives it. It never reserves or allows space from a size that the
archive declares. A decoder that gives more bytes than the entry declares is a
corrupt archive, as legacy `zip.ts` treated a size mismatch.

**Free space.** korrid has no free-space check today. A search of
`services/korrid` for `statvfs`, `statfs`, `free space`, `available_space`,
`fs2` and `fs4` finds nothing. Legacy product code has none either. `libc` is
a direct dependency of korrid, so `libc::fstatvfs` needs no new crate. The
rule: before the download, before the unpack and after each written block,
korrid reads the space available to its own user on the cache's file system
(`f_bavail × f_frsize`). If that space is below a reserve, korrid stops, deletes
the work folder and the archive, and fails with the existing `NoSpace` copy
("There is not enough space for this download."). korrid runs as the system
user `korrid` (`nixos-module.nix`), so `f_bavail` already excludes blocks that
the file system keeps for root. The reserve value is owner question 2. The
reason a reserve matters: the default storage root `/var/lib/korri` and the
private state root `/var/lib/korrid` sit on the device's root file system, so
a full cache also stops library writes, saves and system updates. This is
inferred from the module defaults, not checked on a device.

**Peak disk use.** The archive and the unpacked folder exist together until
publication. Peak use is the archive size plus the unpacked size. The free
space rule covers this, because it measures the file system, not a budget.

**Archive bombs.** A small archive can expand to fill the disk. The free
space rule stops that. It does not stop the time and storage wear of writing
until the reserve. A ratio guard does: korrid stops when the bytes written
exceed R times the archive's size. R has no grounding in Korri or legacy. The
only fixed fact found is the format limit for deflate, 1032:1
(<https://zlib.net/zlib_tech.html>). LZMA has no such low limit. ISOs padded
with zeros can compress very well, so a low R can refuse a real game (inferred,
not measured). Owner question 3.

**Entry count and depth.** korrid uses discovery's `TraversalBudget` default
(`discovery/scanner.rs`): 100,000 entries, 10,000 folders, depth 32, and
10,000 entries in one folder. These are not new numbers. Identification
cannot see past them anyway, so an archive above them can never become a
release. korrid counts entries as it reads the list and fails before it
writes. "Thousands of files" is within the budget.

**Memory.** A 7z or rar decoder allocates a dictionary whose size the archive
sets. `sevenz-rust2` 0.24.1 passes no memory limit to its LZMA decoders
(`src/decoder.rs`, `LzmaReader::new_with_props(..., None)` and
`Lzma2Reader::new(..., None)`). Its `Archive.blocks`,
`Coder::encoder_method_id` and `Coder::properties` are public, so korrid can
read each coder's dictionary size before it decodes. korrid refuses an
archive whose dictionary exceeds `MemAvailable` in `/proc/meminfo`. That is a check grounded in the device.
`rars` has `with_rar50_dictionary_size_limit` for the same check. korrid has
no `/proc/meminfo` reader today. If the extractor runs out of process
(owner question 1), a systemd `MemoryMax=` on the unit would also contain an
allocation that the check misses. Its value is unresolved.

**Time.** Unpacking a large 7z on a handheld can take minutes. The job keeps
its deep-sleep block (ADR 0013) while it unpacks.

### Formats and crates

No dependency is added by this ADR. Facts from crates.io, the crate sources
and NVD, read on 2026-10-23:

| Crate | Version | Licence | Code | Facts that matter here |
|---|---|---|---|---|
| [`zip`](https://crates.io/crates/zip) ([zip-rs/zip2](https://github.com/zip-rs/zip2)) | 9.0.5, 2026-10-17 | MIT | Rust | Reads zip64, so entries over 4 GiB work. Has `enclosed_name`, `is_symlink` and `unix_mode` per entry. Default features pull `zstd` (C) and others, so korrid would use `default-features = false` with deflate only. Its `extract` can create symbolic links, so korrid must not call it. Active: pushed 2026-10-23. |
| legacy `zip.ts` | | | TypeScript | Reads the whole archive into memory. Reads only 32-bit sizes and a 16-bit entry count, so no zip64. It cannot carry an ISO over 4 GiB. Porting it is not enough for the owner's scope. |
| [`sevenz-rust2`](https://crates.io/crates/sevenz-rust2) ([hasenbanck/sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2)) | 0.24.1, 2026-10-18 | Apache-2.0 | Rust | Maintained fork of the unmaintained `sevenz-rust` (last release 0.6.1, 2024-07-17). Decodes COPY, LZMA, LZMA2, BZIP2 and PPMD by default, and DEFLATE, BROTLI, LZ4 and ZSTD behind optional features (`zstd` is C). Filters BCJ x86, ARM, ARM64, ARM_THUMB, RISC_V, PPC, SPARC, IA64, BCJ2 and DELTA. 43 releases. No memory limit on decoders (see "Memory"). Uses `lzma-rust2` 0.23 (Apache-2.0). |
| [`rars`](https://crates.io/crates/rars) ([bitplane/rars](https://github.com/bitplane/rars)) | 0.11.0, 2026-10-16 | Apache-2.0 | Rust, `unsafe_code = "forbid"` | Reads RAR 1.4 to RAR 7 and also writes. Has dictionary, header and output limits. Young: first release 2026-05-13, 30 releases, about 110,000 lines. Its README says it "started as an agentic development experiment" and "could use more testing at volume". korrid would disable the default `write`, `parallel`, `recovery` and `encryption` features. |
| [`unrar`](https://crates.io/crates/unrar) / `unrar_sys` | 0.5.8, 2025-02-19 | crate: MIT OR Apache-2.0. Bundled source: UnRAR licence. | C++ (RARLAB UnRAR 7.1.0, 2024-05-12) | See "The UnRAR licence". Repository last pushed 2025-04-14. NVD lists memory-safety CVEs in UnRAR from 2017 to 2026, the last CVE-2026-14191 (RAR5 recovery volumes, 2026-07-01). korrid would never open `.rev` sets, but the bundled source lags RARLAB's releases. |
| [`rar`](https://crates.io/crates/rar) | 0.4.0, 2025-11-05 | MIT | Rust | Extracts only stored (uncompressed) RAR 5 entries. Not usable. |
| [`compress-tools`](https://crates.io/crates/compress-tools) | 0.16.1, 2026-04-23 | MIT OR Apache-2.0, links libarchive (BSD-3-Clause in nixpkgs, 3.8.9) | C | Reads zip, 7z and rar 4 and 5 with one library. NVD lists 2026 CVEs in libarchive's RAR and RAR5 readers: CVE-2026-4111, CVE-2026-4424, CVE-2026-14164. |

**The UnRAR licence** (`unrar_sys/vendor/unrar/license.txt`). Its terms:
the source "may be used in any software to handle RAR archives without
limitations free of charge, but cannot be used to develop RAR (WinRAR)
compatible archiver and to re-create RAR compression algorithm". Modified
source may be distributed if that paragraph is kept in the licence or
documentation and in source comments. The licence has no platform limit, so
ARM builds are allowed. Debian ships it in `non-free` (`unrar-nonfree`
7.3.1), and nixpkgs marks `unrar` 7.3.1 `unfreeRedistributable`. The use
restriction is an extra condition that the GPL does not allow, so GPL code
cannot link it (inferred, my reading, not legal advice). MIT code can
include it if the paragraph travels with it. Korri has no licence file in
this repository today, so the question is open. Opinion: the licence alone
is a reason to prefer `rars` or libarchive.

**Where untrusted C or C++ would run.** With `unrar` or `compress-tools`
in korrid's process, a crafted archive parses in C or C++ inside the process
that holds the device key, signs as the device, and writes the library.
A memory bug there runs as the `korrid` user with all of that. With pure Rust
crates the same bug class is mostly gone, but a parser panic, a large
allocation or a long decode still hits korrid itself. The 7z dictionary case
above is one.

**Out of process, still core.** korrid can run the extractor as a korrid-owned
program in the ADR 0012 sandbox unit. `acquisition/unit.rs`
`unit_arguments` already gives that unit no network
(`PrivateNetwork=yes`), no capabilities, `ProtectSystem=strict`, the
contribution folder as its only writable path, and the private state root as
inaccessible. The unit name for a provider job follows the cache grammar:
`korri-acquire-<plugin short>-<provider name>.service`. That cannot collide
with an artifact's unit, because declaration evaluation already refuses an
artifact key that equals a provider name (ADR 0016, "Consequences"). The
extractor can be korrid's own binary in a separate mode, with the same Rust
crates, so no third-party program enters the closure. Or it can be `bsdtar`
from libarchive, which brings C code into the sandbox, not into korrid.
Cost: one process start per unpack, a second code path for the unit's
lifecycle (ADR 0012 already has it), and progress that korrid must read from
the work folder instead of from a loop.

### Change 1c: the `Unpacking` job state

Owner answer Q1e. `AcquireJobState` gains `{ _tag: "Unpacking" }`, with no
payload. This is ADR 0012's artifact state name and shape
(`ArtifactState::Unpacking` in `acquisition/types.rs`). The Typeshare output
changes through `nix run .#korrid-check`. `acquire` treats `Unpacking` as a
live job, like `Placing`.

Cancel: ADR 0012 says "Unpacking cannot be cancelled" for artifacts. For a
store job, korrid has published nothing while it unpacks, so cancel can stop
the unpack and delete the work folder and the archive. Proposal: `Unpacking`
can be cancelled for a store job. ADR 0012 artifacts keep their rule. Cost:
the unpack loop or the unit must check for cancel, and the two kinds of job
then differ.

### Change 2: a form POST on `FinalDownload`

`FinalDownload` gains two optional fields, `method` and `body`. korrid sends
the game download with them. Provider turns stay GET only, because the
WoWROMs link step works as a GET.

Grounding: legacy `product/platform/plugin/services.ts` declares
`method?: string` ("HTTP method. Defaults to GET.") and
`body?: string | Uint8Array | URLSearchParams` ("Request body for
POST/PUT/PATCH."). Legacy `plugin-runtime.ts` sent them through
`fetchWithValidatedRedirects`. The legacy parking-lot item
`01KXC8EAAD647X5PBTYMP06T6E` names this gap: "PluginHttpRequestOptions
supports method + body (POST) so wowroms can submit its download form".
Legacy carried the two fields on request options, not on `FinalDownload`.
Moving them to `FinalDownload` is the delta.

Coordinator decisions, each with its reason:

- **Q2a, shape.** `method` is `"POST"` or absent (GET). `body` is an object
  of string values. korrid form-encodes it and sends
  `Content-Type: application/x-www-form-urlencoded`. Reason: JSON can carry
  legacy's `URLSearchParams` form only as such an object, and the WoWROMs
  POST is that form. `FinalDownload.requestHeaders` stays dropped. A `body`
  without `method: "POST"` fails admission.
- **Q2b, redirects.** 301, 302 and 303 after a POST become a GET with no
  body. 307 and 308 send the same POST again to the next declared hop.
  Reason: this is what browsers do (RFC 9110 section 15.4), and the sites
  were built for browsers.
- **Q2c, approval.** No separate declaration. Reason: the POST reaches only
  hosts that the person approved at install, at most once per acquire.
- **Q2d, bound.** The form-encoded body is at most 8192 characters, the
  existing legacy `SafeText` bound (`claims.rs` line 13). Reason: it is the
  existing bound for plugin text, and the WoWROMs body is three short fields.

This amends two ADR 0016 "Grounding and deltas" rows:

- `PluginHttpRequest { url, query?, headers?, timeoutMs? }`: "GET only:
  `method` and `body` are not carried (no in-scope provider posts)". This
  stays true for provider turns. It no longer holds for the game download.
- `ProviderClaim`, `ProviderClaimDetails`, `ArtifactAcquisitionHint`,
  `DownloadResolution`, `DownloadChoice`: `FinalDownload` gains `method` and
  `body`.

**Security effect.**

- **At most one POST per acquire,** plus 307 and 308 repeats on declared
  hops. `fetch_game` makes one attempt and never retries or resumes a store
  download (ADR 0016).
- **Only to declared hosts.** `policy::declared` checks the URL and every
  redirect hop, as today. The SSRF policy applies to every hop.
- **Still no cookie or authorization.** `cookie`, `set-cookie`,
  `authorization`, `proxy-authorization` and `host` stay refused. The only
  header added is the form content type.
- **Failure copy never contains the body.** Diagnostics stay at
  `log::debug!`, which the build compiles out.
- **Side effects.** A POST can change state on the server. The scope is the
  hosts that the person approved at install. The approval report shows hosts,
  not methods.

### Change 3: the name of the downloaded file

This amends ADR 0016 owner answer Q1, which names the ready file
`<URL basename>`. It also reverses the comment on `policy::basename`
(`policy.rs` line 106): "Names come from the declared URL, not a
server-controlled redirect or header." The new name still does not come from
the server.

Coordinator decision Q3a, N1: the outer name is `FinalDownload.filename`,
then the URL's last segment. Reason: `filename` is already on the wire and
admitted as `SafeFileName` (`claims.rs` `DownloadResolution::admit`), and
legacy used the same order (`resolution.filename ?? request.fileName ??
downloadUrlBasename(resolution.url)`, `artifact-acquisition.ts`) without the
middle term, which ADR 0016 keeps out of the portal's reach. `safe_file_name`
allows control characters and `policy::basename` refuses them, so korrid
applies both rules.

With Change 1 the outer name has two jobs:

- It passes the check before the download (a system or archive extension).
- For a plain download, it is the file's name in the ready folder. For an
  archive, it names nothing in the cache. The ready folder holds the
  archive's entries under their own paths.

The ready folder becomes
`cache/<plugin short>/<provider name>/<sha256 hex of the release identity>/`
with the unpacked tree inside. Only the content of the folder changes. The
rest of ADR 0016 Q1 stays.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| Unpacking is a core capability | Owner answer Q1a | Replaces the first draft's three options. ADR 0012's per-plugin `unpack` program stays for formats korrid does not read. |
| Formats zip, 7z, rar | Owner answer Q1b, legacy `ARTIFACT_MAGIC` | The three magic rows come back. `gz`, `tar` and `png` stay out. |
| Archive is transport, never a release | ADR 0005 identity rule, ADR 0013 "hashes of its main content file" | Amends ADR 0005 "Archives are not supported in the first version" for downloads into korrid's cache. Discovery of sources still refuses archives. |
| Identification of the unpacked folder | `identify_with_rules`, `ContentRules`, `TraversalBudget` (discovery v2) | New caller. The default layout is tried after the system's layouts. Exactly one unit is required. |
| Release identity kinds | `ContentIdentity` and `LibraryIdentity` `Hash`, `Parts`, `Chd` | `record_acquired` writes `Hash` or `Parts`. `Chd` is unresolved, because its value does not fit the `<sha256 hex>` folder grammar. |
| Main file of an application archive | ADR 0013 `file: { artifact, path }`, ADR 0016 `packageFile`, legacy PortMaster `isLaunchScript` | No new schema for declared games. Unresolved for provider downloads. |
| Safe extraction rules | Rock N Roll `unpack.py`, legacy `safeZipPath`, `O_NOFOLLOW` staging | Moved from one plugin program into korrid. |
| File modes `0644` and `0755` | Rock N Roll `unpack.py` default | No executable bits from archives. |
| Entry, folder and depth limits | discovery `TraversalBudget::default()` | Reused, not new. |
| Free-space check | none in korrid or legacy. `libc` is a direct dependency. `Failure::NoSpace` copy exists. | New mechanism. Reserve unresolved (owner question 2). |
| 2 GiB download cap | legacy `MAX_FALLBACK_DOWNLOAD_BYTES`, ADR 0016 | Proposed for removal in favour of the free-space check (owner question 2). `TooLarge` copy goes with it. |
| Ratio guard | zlib's 1032:1 deflate limit, as a fact only | Number unresolved (owner question 3). |
| Dictionary memory check | `/proc/meminfo` `MemAvailable` | New. No korrid reader exists today. |
| Legacy zip reader and its limits | legacy `zip.ts`, gmloader defaults | Not carried: no zip64, whole archive in memory. Its fixed caps are replaced by the device checks above. |
| `Unpacking` job state | Owner answer Q1e, ADR 0012 `ArtifactState::Unpacking` | New `AcquireJobState` variant. Cancellable for store jobs. |
| Sandbox unit for a provider job | `unit.rs` `unit_arguments`, ADR 0016 artifact key and provider name rule | Only if owner question 1 picks out of process. |
| `FinalDownload.method`, `FinalDownload.body` | legacy `PluginHttpRequestOptions.method`, `.body` | Moved to `FinalDownload`. Coordinator decisions Q2a to Q2d. |
| Redirect after POST | legacy `fetchWithValidatedRedirects`, RFC 9110 section 15.4 | Browser rules (Q2b). |
| Outer name order | legacy `resolution.filename ?? request.fileName ?? downloadUrlBasename(...)` | N1 drops `request.fileName` (Q3a). |
| Failure copy | `acquisition/types.rs` `Failure`, code `AcquisitionFailed` | No new wire code. `Unpack`, `NotSystemFile` and `NoSpace` cover the new failures. |

## Owner questions

1. **Where does the archive reader run?** Everything here is korrid core. The
   choice is whether a crafted archive is read inside the korrid process, which
   holds the device key and writes the library, or in a separate sandboxed
   process that korrid starts and that can write only its own work folder.

   | Option | RAR reader | Cost |
   |---|---|---|
   | A. In korrid's process, pure Rust | `rars` | No C or C++. A reader bug can still crash korrid or use all its memory. `rars` is five months old. |
   | B. korrid's own code, in the ADR 0012 sandbox unit | `rars` | Same crates as A. A crash or memory blow-up ends only the extractor. One process start per unpack. |
   | C. `bsdtar` (libarchive), in the sandbox unit | libarchive | One mature C library for all three formats. C parses untrusted input, but inside the sandbox. libarchive had RAR5 CVEs in 2026. |
   | D. In korrid's process with `unrar` | UnRAR (C++) | Not recommended. C++ in korrid's process, a non-free licence, and a bundled source from 2024. |

   Lean (opinion): B. It keeps all parsing in Rust and still keeps a bad
   archive away from the device key.

2. **How much free space must stay free, and does the 2 GiB download cap go?**
   korrid will stop a download or an unpack when the space left on the
   cache's disk falls below a reserve, and delete what it wrote. The cache
   shares its disk with saves, the library and system updates. Options: no
   reserve beyond what the file system already keeps for root, a fixed size,
   or a percentage of the disk. The same check can replace the 2 GiB cap that
   ADR 0016 took from legacy. Disc games and ISOs exceed 2 GiB.

3. **Do you want a compression-ratio guard, and at what ratio?** A tiny
   archive can claim to expand to terabytes. The free-space check already
   stops it at the reserve, but only after writing until the disk is nearly
   full, which takes time and wears an SD card. A ratio guard stops earlier,
   when the written bytes reach R times the archive's size. Zip's deflate
   cannot exceed 1032:1. Disc images padded with zeros can compress very well,
   so a low R can refuse a real game. Options: no guard, or a value of R.

4. **Do declared artifacts (ADR 0012 and ADR 0013) also get core
   unpacking, and how do they ask for it?** Today a pinned artifact with no
   `unpack` program lands as the downloaded file, unchanged. Mega Man Rock N
   Roll ships its own ZIP extractor. Options:
   - (a) Not now. Only store downloads use core unpacking.
   - (b) Implicit: korrid unpacks an artifact with no `unpack` program when
     its bytes are zip, 7z or rar. No new field, but an artifact that must
     stay an archive (for example an arcade set that an emulator reads as a
     zip) can no longer be declared.
   - (c) Explicit: a new declaration field. No existing shape grounds its name
     or form, so the real artifact that needs it must define it.

   In every option, a plugin whose files need executable bits keeps its own
   unpack program, because korrid gives no file an executable bit.

5. **Are application archives from a store provider in scope now?** For a
   declared game, the plugin already names the main file with
   `file: { artifact, path }`. For a store download, nothing names the file
   that starts the game, and ADR 0016 excluded itch.io ("skip itchio"). If
   yes, which real provider plugin defines where it names the main file
   and the release hash? Until then, a store download must hold ROM or disc
   content that discovery's system rules can identify.

## Considered options

- **Keep one content file from the archive (first draft).** It fits one ROM
  per archive. It cannot carry a cue with its tracks, a multi-disc set or an
  application folder, which the owner's answers require.
- **A new algorithm to find "the game file".** The owner noted there is no
  deterministic way. Discovery's rules are the existing answer for ROM and
  disc content, and a declaration is the existing answer for applications.
- **Keep the archive whole and let the runner open it (legacy).** Legacy
  RetroArch discovery claimed `.zip` and `.7z` for cart systems
  (`product/plugins/retroarch/src/discovery.ts` lines 92–144). The release
  identity would then be the archive's bytes, and the same game from two
  sites would make two releases. ADR 0005 and the discovery v2 brief refuse
  this.
- **Each provider plugin ships an unpack program (ADR 0012 style).** The owner
  chose core. Provider plugins are source only today
  (`nix/product/PUBLISHED-PLUGINS.md`), so each would need a native closure
  per architecture.
- **Port legacy `zip.ts`.** No zip64 and whole-archive reads, so no ISO over
  4 GiB.
- **The plugin fetches the bytes (`artifact.acquire`).** ADR 0016 dropped this
  operation, because it needs I/O in the sandbox.
- **Do nothing.** ROM-site providers stay refused.

## Consequences and stated limits

- **Out of scope:** nested archives, split and multi-volume archives,
  encrypted archives, `gz` and `tar` for store downloads, CHD inside an
  archive, arcade sets where the zip is the content, `kind: "patch"` claims,
  and application archives from providers (owner question 5).
- **Discovery is unchanged.** An archive in a source is still not a game.
- **One release per download.** An archive with several games fails. A
  multi-disc set that the system's rules cannot combine fails. Sites that
  ship each disc as its own download give one release per disc.
- **Disk.** Peak use is the archive plus its contents. The free-space reserve
  protects the rest of the device.
- **A second full read.** Identification reads every content byte after the
  unpack wrote it. For a 4 GiB ISO that is 4 GiB more reading on the device.
- **Signed links.** A store job resolves only when its queue turn starts
  (`game.rs`, around line 560), so coolrom's link is fresh when the download
  starts. Whether coolrom stops a transfer that runs past about 300 s is
  unknown.
- **Host pools.** WoWROMs picks its file host from a pool. A host outside the
  plugin's `hosts` fails as `UndeclaredHost` until the plugin is updated and
  approved again.
- **Content.** These sites distribute games that the person does not
  necessarily own. Korri adds no such provider. ADR 0016 owner decision 2
  still lists Korri's own providers. The private `@simonwjackson` plugins and
  their use are the owner's matter. Legacy `PluginMetadata.legalRisk` is not
  carried (ADR 0016).
- **Not checked on a device.** No change in this ADR has run on hardware.

## What an implementation changes

No code changes with this ADR. Line numbers are from main at `ad3da260f`.

| File | Place | Change |
|---|---|---|
| `services/korrid/src/acquisition/game.rs` | lines 603–616 | Outer name (N1) and the check before the download (system or archive extension) |
| same | `fetch_game` call, line 644 | Pass `method` and `body` |
| same | line 662 and `publish_and_record` | `Unpacking` state, unpack, identify the work folder, publish, record the `ContentIdentity` |
| same | `acquired_game_dirs`, `acquired_game_content` | Main-file rule for a folder with several files |
| `services/korrid/src/acquisition/download.rs` | `send` (line 163, `client.get` at 174) | Method, form body, redirect rule (Q2b) |
| same | `fetch_game` (line 257), `MAX_GAME_BYTES` (line 43) | Free-space check in place of the cap, if owner question 2 says so. Keep the lead bytes for magic detection. |
| `services/korrid/src/acquisition/claims.rs` | `FinalDownload` (line 187), `admit` (line 424) | `method`, `body`, the 8192-character bound, the stricter `filename` check |
| `services/korrid/src/acquisition/policy.rs` | lines 106–140 | The name rule and its comment |
| `services/korrid/src/acquisition/placement.rs` | `publish_acquired_game` (274), `ready_game_file` (310) | Publish a folder. Find the main file. Startup removes leftover work. |
| `services/korrid/src/acquisition/types.rs` | `AcquireJobState` (191), `Failure` (265) | `Unpacking`. Drop `TooLarge` if the cap goes. |
| `services/korrid/src/acquisition/unit.rs` | `name`, `unit_arguments` | Only for owner question 1, options B and C: unit name for a provider job, `MemoryMax=` |
| new module under `services/korrid/src/acquisition/` | new | Archive reader, safe extraction, free-space and memory checks |
| `services/korrid/src/discovery/reconcile.rs` | `AcquiredGame`, `record_acquired` (413) | Carry and write `Hash` or `Parts` |
| `services/korrid/src/discovery/identify/mod.rs` | `identify_with_rules` | A caller for a work folder, with the default layout after the system's layouts |
| `services/korrid/Cargo.toml` | `[dependencies]` | The crates that owner question 1 picks |
| `services/korrid/src/acquisition/provider_tests.rs` | lines 136, 178, 774–777 | The `zip` fixture becomes archive tests: each format, magic mismatch, cue with tracks, `.m3u` set, several games, unsafe entries, free space, ratio |
| `services/korrid/src/acquisition/game_download_tests.rs`, `turns_tests.rs` | new tests | POST, redirect after POST, `filename` naming |
| `services/korrid/SCRIPTING.md` | "Request and response turns", "Results", "A provider download stays in korrid's cache", "Declared artifacts" | The new fields, states and rules |
| `contracts/generated/korrid.ts` | whole file | Regenerated by `nix run .#korrid-check`, never by hand |
| `docs/adr/0005-release-identity.md`, `docs/adr/0012-korrid-acquires-declared-artifacts.md`, `docs/adr/0016-korrid-acquires-games-from-provider-plugins.md` | amendment notes | On acceptance |
