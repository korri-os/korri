---
status: proposed (ready for review)
---

# korrid unpacks archives in its core

**Ready for review. No open owner questions.** Every choice below is an owner
answer, a coordinator decision, or a proposal that follows from existing code
and says where it comes from.

**Owner answers, 2026-10-23, first round.**

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

**Coordinator decisions, 2026-10-23, first round.** The owner asked "Does it
matter?" for Q3a and did not have the context for Q2a to Q2d. These are
implementation details. "Change 2" and "Change 3" give one line of reason for
each.

- Q2a: `body` is an object of string values. korrid form-encodes it and sets
  `application/x-www-form-urlencoded`. POST is the only method besides the GET
  default.
- Q2b: browser rules. 301, 302 and 303 become a GET with no body. 307 and 308
  send the POST again.
- Q2c: no separate declaration. The approved `hosts` stay the boundary.
- Q2d: the existing `SafeText` bound, 8192 characters.
- Q3a: N1. `FinalDownload.filename`, then the URL's last segment.

**Owner and coordinator answers, 2026-10-23, second round.**

1. Owner: the reader is korrid's own pure-Rust code, run as a sandboxed
   subprocess in the ADR 0012 sandbox unit. Crates: `zip` with default
   features off and no symbolic-link extraction, `sevenz-rust2`, and `rars`
   for RAR. If `rars` cannot read a file, that download fails with the
   existing unpack failure copy. No UnRAR, no libarchive, no C or C++ reader.
2. Coordinator, from the owner's "games can be very big": no 2 GiB cap
   (`MAX_GAME_BYTES`) for store downloads or their unpacked result. korrid
   counts bytes as it writes and stops before the cache's file system falls
   below 5% free. korrid keeps the `MemAvailable` check against the 7z or rar
   dictionary size.
3. Coordinator: no compression-ratio guard.
4. Coordinator, a scope call: core unpacking for ADR 0012 and ADR 0013
   declared artifacts is not in this ADR. See "Later".
5. Owner: application archives come later. This ADR covers single ROMs, disc
   images with their cue files, and multi-disc sets with an `.m3u`. See "Out
   of scope".

**The decision.** A store download that is a zip, 7z or rar archive is
unpacked by korrid's own Rust reader in a sandboxed unit. The reader refuses
unsafe entries before it writes anything. korrid then identifies what is
inside with the rules discovery already has: the system's content rules and
header rules, and ADR 0005's multi-file identities. korrid adds no new rule to
find "the game file". The archive is never a release. The release is what
discovery would find if the same files were in a source, so a download and a
copy in a source join one game. Disk use is bounded by free space, memory by
`MemAvailable`, and entries by discovery's existing traversal budget.

## Changes on main since the first draft

- `cdc330776` ("identify acquired releases by content hash") fixed the first
  draft's Q4. The acquire job uses discovery's ADR 0005 identity, header
  excluded (`acquisition/game.rs` `publish_and_record`,
  `identify::identity_of`). This ADR extends that call from one file to a
  folder.
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

The owner's answers widen the problem. A game is not always one ROM. It can be
a disc image with its cue sheet, a multi-disc set, or an ISO of several GiB. A
model that keeps "one content file" from an archive does not fit.

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
  romhustler `.rar`, 543,960,550 bytes. With this ADR they are in scope if
  they hold a cue with its tracks, an `.m3u` set or one CHD.
- The legacy parking-lot item `01KXC8EAAD647X5PBTYMP06T6E` (2026-07-12) said
  romhustler guest downloads were account-gated, coolrom needed a cookie jar,
  and WoWROMs needed a POST after a JavaScript countdown. The 2026-10-23
  evidence replaces the first two: 2 of 5 romhustler games download as a
  guest, and coolrom sent no `Set-Cookie`. It confirms the POST.

## Scope

- **In:** store downloads (ADR 0016) that are a single ROM, a disc image with
  its cue file, a CHD, or a multi-disc set with an `.m3u`, either plain or in
  a zip, 7z or rar archive.
- **Not in this ADR:** declared artifacts (see "Later"), application archives
  (see "Out of scope"), and archives in a person's sources. Discovery is
  unchanged: an archive in a source is still not a game, and korrid never
  writes into a person's source (ADR 0013).
- The first consumers are the private `@simonwjackson` provider plugins.
  ADR 0016 owner decision 2 still lists Korri's own providers. This ADR adds
  no ROM-site provider to Korri.
- Plugins stay effect-free. korrid makes every request, downloads, unpacks
  and writes the record (AGENTS.md "Federation" and "Plugin boundaries"). The
  unpack runs on the device whose korrid runs the store
  ([ADR 0017](0017-the-device-korrid-runs-discovery-and-the-store.md)).

## Decision

### Change 1: korrid unpacks the whole archive

A store job does these steps:

1. It downloads as today: declared hosts, the SSRF and redirect policy, one
   attempt, and the web page and empty-body refusal. The 2 GiB cap is gone.
   The free-space floor in "Limits" applies instead.
2. It decides from the first bytes whether the download is an archive, and
   which format. The legacy `ARTIFACT_MAGIC` rows come back: zip is
   `50 4b 03 04`, `50 4b 05 06` or `50 4b 07 08`, 7z is `37 7a bc af 27 1c`,
   rar is `52 61 72 21 1a 07`. The name alone is not enough: romhustler sends
   `.7z` and `.rar` bytes under a `.zip` name.
3. The job state becomes `Unpacking` (Change 1c).
4. korrid creates a `.tmp-*` work folder in the provider cache and starts the
   extractor in the sandbox unit (Change 1d). The extractor reads the whole
   entry list and checks every entry before it writes anything. One bad entry
   refuses the whole archive. Then it writes every entry into the work folder.
5. korrid identifies the work folder (Change 1b). The result must be exactly
   one release.
6. korrid deletes the archive, freezes and renames the work folder to the
   ready folder, and writes the release with the identified identity.

A download whose bytes are not an archive takes the same path without the
extractor. korrid moves the file into the work folder under its name
(Change 3) and runs the same identification. Plain downloads and archives
share one rule.

Before the download, korrid accepts a name that ends in a system extension or
in `.zip`, `.7z` or `.rar`. It refuses any other name before the download, as
today. After the download, the bytes decide. A name that says archive with
bytes in no archive format fails as `NotSystemFile`.

Nested archives, split and multi-volume archives (`.7z.001`, `.part1.rar`,
`.rev` recovery sets), encrypted archives, and any file that the chosen
reader cannot read fail with the existing `Unpack` copy ("The downloaded files
could not be unpacked.").

### Change 1b: korrid identifies the content with discovery's rules

**The existing function.** `discovery::identify::identify_with_rules`
(`discovery/identify/mod.rs`) is the "core seam for registry-independent
consumers and tests". It takes any folder as the root of a `SourceInput`, a
list of `ContentRules`, a `TraversalBudget` and a `HashCache`. It walks the
folder, applies extensions, layouts and header rules, lets `.m3u` own its
discs and `.cue` own its tracks, reads CHD headers, and returns `FoundUnit`s
with a `ContentIdentity`. Its unit tests already run it over plain temporary
folders (`identify/tests.rs`, for example
`cue_tracks_form_one_ordered_identity_and_are_not_separate_units`). korrid
runs it over the work folder with the claim system's rules, `group = false`,
no assignments and no declared-hash lookups. This is verified by reading the
code and its tests. No test runs it over an acquisition work folder yet.

**Accepted results.** The folder must give exactly one unit and no attention
finding (no `Unmatched`, `Unreadable` or `Clash`). Each row follows from
`identify/disc.rs` and `identify/mod.rs` as they are on main.

| The download holds | Units | Release identity | Result |
|---|---|---|---|
| One ROM, maybe with a readme or `.nfo` | 1 | `Hash`: SHA-256 and SHA-1 of the ROM, header excluded | Accepted. Files with no system extension are ignored. |
| A disc image with its cue (`Game.cue`, `Game (Track 1).bin`, ...) and a system that declares `cue` | 1 | `Parts`: SHA-256 over the ordered track hashes. The cue is not hashed. | Accepted. |
| A multi-disc set with an `.m3u` | 1 | `Parts`: over the disc identities in playlist order | Accepted. |
| One ISO (`psp` declares `iso`) | 1 | `Hash` of the whole ISO | Accepted. |
| One CHD | 1 | `Chd`: the header's raw SHA-1, `sha1:<40 hex>` | Accepted. See "The ready folder name". |
| A multi-disc set with no `.m3u` | one per disc, or 1 | | Refused, see "Multi-disc sets without an `.m3u`". |
| Several games | 2 or more | | Refused as `NotSystemFile`. |
| An archive inside the archive, or no file with a system extension | 0 | | Refused as `NotSystemFile`. |

A copy of the same files in a paired source gets the same identity, because
discovery computes it with the same function. So the store's release and the
discovered copy join one game, as ADR 0005 requires. This is inferred from the
code, not tested end to end.

**One release per job.** A store job writes one game, one release and one
provider link (ADR 0016 owner answer Q2). So the folder must hold one unit.
Cost: an archive that bundles several regions of one game fails. So does a
file that only looks like a game: `md` is a Mega Drive extension, so a
`README.md` beside a Mega Drive ROM is a second unit, as the test
`a_readme_in_a_paired_mega_drive_source_is_claimed_by_the_md_extension`
shows. The person can still put such files into a source.

**Layouts.** A system's layouts describe a person's folder, not an archive.
`layout::match_layout` uses the system's layouts, or `**/{title}.{ext}` only
when the system declares none, and takes the first match. If no layout
matches an entry path, discovery reports `Unmatched`. Proposal: for a work
folder, korrid passes a copy of the claim system's rules with
`**/{title}.{ext}` appended after its layouts. This is the existing default
layout (SCRIPTING.md, "Content, groups and declared games"), so no new pattern
is added. A path that a system layout matches keeps that layout's captures.
The title capture is not used: the claim's title names the game (ADR 0016
owner answer Q2). Cost: a work folder accepts paths that a source scan of the
same system reports as `Unmatched`.

**Multi-disc sets without an `.m3u`.** The scope is sets with an `.m3u`.
Proposal: korrid refuses a folder with more than one disc and no `.m3u`, and
accepts a set that the system's own `{disc}` layout combines only through that
layout, with no rule of its own. Discovery's `combine_discs` joins discs only
when a layout captured `{disc}`. The appended default layout has no `{disc}`,
so discs that no system layout matches stay separate units and fail the
one-unit rule. No extra check is needed. Cost: a site that ships a multi-disc
game as one archive without an `.m3u` gives no release. Sites that ship each
disc as its own download give one release per disc.

**The release record.** `AcquiredGame` carries `sha256_hex` today, and
`record_acquired` writes `LibraryIdentity::Hash`. It carries the
`ContentIdentity` instead and writes `Hash`, `Parts` or `Chd`. All three kinds
already exist in `config/library.rs` `LibraryIdentity`. No new schema.

**The ready folder name.** ADR 0016's "Grounding and deltas" row for the ready
folder says: "The version segment is discovery's content hash (ADR 0005)". The
content hash is the release key, `ContentIdentity::value()` ("The release key:
`locations` key and catalog identity value"). For `Hash` and `Parts` it is
`sha256:<64 hex>`. For `Chd` it is `sha1:<40 hex>`, because ADR 0005
identifies a CHD by its stored raw SHA-1. Proposal: the version segment is the
release key's hex digits without the algorithm prefix. A 64-digit name is a
SHA-256 key and a 40-digit name is a CHD SHA-1 key, so the two kinds cannot
collide. `acquired_game_dirs` (`game.rs`) strips `sha256:` or `sha1:` from the
release key, not only `sha256:`. Cost: ADR 0016 Q1 text says `<sha256 hex>`
and needs an amendment note.

**The route.** `placement::ready_game_file` accepts a ready folder with exactly
one top-level regular file. That stays true for one ROM, one ISO or one CHD.
A cue with its tracks, or an `.m3u` set, puts several files in the folder, and
an archive can put them in subfolders. Proposal: the route walks the ready
folder and takes the files whose names end in one of the release system's
extensions. From those it takes the one `.m3u`, else the one `.cue`, else the
one file. This is discovery's owner order (`identify_files` visits `m3u`,
then `cue`, then other files), and it gives the unit's `FoundUnit.path`.
Publication already required exactly one unit and no attention finding, so
the walk finds exactly one candidate. The route still hashes nothing on
launch. No location is saved, as ADR 0016 owner answer Q3 says. Cost: a
folder walk on each route resolution.

**Plain downloads.** The staging file is named `.acquire-<hex>`. Rust's
`Path::extension` returns nothing for that name, so `identity_of` hashes every
plain download as ordinary bytes today. A plain `.chd` download therefore gets
a whole-file SHA-256, while discovery gives the same file a `Chd` identity,
and the two never join. Identifying the file under its real name in the work
folder removes this difference. It also makes a lone `.cue` fail, which is
correct: its tracks are missing. A ready folder that an earlier build made for
a plain CHD under its whole-file SHA-256 is no longer found by the route. The
next Get downloads the game again into the new folder. Startup recovery
already logs a ready folder that no release names and deletes nothing
(ADR 0016, "Consequences").

### Change 1c: the `Unpacking` job state

Owner answer Q1e. `AcquireJobState` gains `{ _tag: "Unpacking" }`, with no
payload. This is ADR 0012's artifact state name and shape
(`ArtifactState::Unpacking` in `acquisition/types.rs`). The Typeshare output
changes through `nix run .#korrid-check`. `acquire` treats `Unpacking` as a
live job, like `Placing`. The job keeps its deep-sleep block (ADR 0013) while
it unpacks.

A store job can be cancelled in `Unpacking`. korrid has published nothing yet,
so cancel stops the unit, the same way `unit::reclaim_with` stops an orphan,
and then deletes the work folder and the archive. ADR 0012 artifacts keep
their rule, "Unpacking cannot be cancelled". Cost: the two kinds of job differ
on this point.

### Change 1d: the extractor and its sandbox unit

**Owner decision.** korrid's own pure-Rust reader runs as a subprocess in the
ADR 0012 sandbox unit. It is korrid's own binary in an extractor mode, so no
third-party program enters the closure. The subcommand name is an
implementation detail. It follows the ADR 0012 unpack-program contract:
`<program> <work-dir> <file>`, exit 0 for success and 1 for failure,
diagnostics on stderr, no stdout contract, no rename of the ready folder.
korrid identifies, renames and records after the program exits.

**The unit.** `acquisition/unit.rs` `unit_arguments` already gives the unit no
network (`PrivateNetwork=yes`), no capabilities, `ProtectSystem=strict`, the
contribution folder as its only writable path (`ReadWritePaths`), and the
private state root as inaccessible. For a store job, the contribution folder
is the provider cache, `cache/<plugin short>/<provider name>`
(`placement::provider_cache`). It holds both the staged archive under
`.downloads` and the `.tmp-*` work folder.

**The unit name.** `unit::name` builds `korri-acquire-<plugin short>-<key>.service`
from an artifact id, where `<plugin short>` and `<key>` are the two segments of
the artifact's cache path `cache/<plugin short>/<key>`. Proposal: a store job's
unit is `korri-acquire-<plugin short>-<provider name>.service`, built from the
two segments of `placement::provider_cache` in the same way. The grounding:

- The unit name follows the cache path in both cases. ADR 0012 already ties
  the unit to the contribution folder that it may write.
- A provider name and an artifact key share one grammar
  (`plugin.rs` `is_provider_segment`: lowercase letters, digits, `.`, `_`,
  `-`), so every name is a valid systemd unit name.
- Declaration evaluation refuses a plugin whose artifact key equals one of its
  provider names, "so ADR 0012 artifact caches and ADR 0016 provider caches
  never share `cache/<plugin>/<key>/`" (ADR 0016, "Consequences"). The same
  rule keeps the two unit names apart.

The `-` separator is ambiguous when a short name or key itself contains `-`
(plugin `a-b` with key `c`, plugin `a` with key `b-c`). ADR 0012 artifact
units have this ambiguity today. It does not cause two live units with one
name, because one korrid runs one job at a time (ADR 0013 queue), and
`unit::unpack` never replaces a unit whose name is already active: it waits
for that unit to stop and then fails the unpack. At startup,
recovery stops any active unit with the name before it deletes `.tmp-*`
folders, as it does for artifacts.

**Crates.** No dependency is added by this ADR. The implementation adds these.
Facts from crates.io and the crate sources, read on 2026-10-23:

| Crate | Version | Licence | Use |
|---|---|---|---|
| [`zip`](https://crates.io/crates/zip) ([zip-rs/zip2](https://github.com/zip-rs/zip2)) | 9.0.5, 2026-10-17 | MIT | `default-features = false`, deflate only. Reads zip64, so entries over 4 GiB work. Its `extract` can create symbolic links, so korrid never calls it. korrid reads entries one by one and uses `is_symlink`, `is_dir` and `enclosed_name` to refuse unsafe entries. |
| [`sevenz-rust2`](https://crates.io/crates/sevenz-rust2) ([hasenbanck/sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2)) | 0.24.1, 2026-10-18 | Apache-2.0 | Maintained fork of `sevenz-rust`. Decodes COPY, LZMA, LZMA2, BZIP2 and PPMD by default, and filters BCJ x86, ARM, ARM64, ARM_THUMB, RISC_V, PPC, SPARC, IA64, BCJ2 and DELTA. The optional `zstd` feature pulls C code and stays off. Uses `lzma-rust2` 0.23 (Apache-2.0). |
| [`rars`](https://crates.io/crates/rars) ([bitplane/rars](https://github.com/bitplane/rars)) | 0.11.0, 2026-10-16 | Apache-2.0 | Reads RAR 1.4 to RAR 7. `unsafe_code = "forbid"`. Has dictionary, header and output limits. Default features `write`, `parallel`, `recovery` and `encryption` stay off. |

Readers that are not used: `unrar` 0.5.8 (C++ UnRAR 7.1.0 under the non-free
UnRAR licence), `compress-tools` 0.16.1 (C libarchive), `rar` 0.4.0 (reads
only uncompressed RAR 5 entries), and a port of legacy `zip.ts` (no zip64, the
whole archive in memory). The first commit of this ADR records the research.

### Limits

The owner called fixed size and entry caps arbitrary. Each check below comes
from the device or from an existing bound.

**Safe extraction.** These rules come from Mega Man Rock N Roll's `unpack.py`
(`aa3840e8f`), legacy `safeZipPath` (`legacy:product/platform/archive/zip.ts`)
and the staging file today (`O_NOFOLLOW` in `download.rs`). The extractor
refuses the whole archive before it writes if one entry:

- is a symbolic link, a hard link, a device, a FIFO or a socket.
- is encrypted.
- has an absolute path, a drive letter, a `..`, `.` or empty segment, a
  backslash, a NUL or another control character.
- repeats a path that another entry already has.

Each path segment must pass legacy `SafeFileName` (`claims::safe_file_name`)
and the control-character rule of `policy::basename`. The extractor creates
every file with `O_CREAT | O_EXCL | O_NOFOLLOW` in a work folder that korrid
made, so a name that collides on a case-insensitive file system fails instead
of overwriting. Every file gets mode `0644` and every folder `0755`, the
default of `unpack.py`. No executable bit comes from an archive. After the
extractor exits, korrid removes write permissions, syncs the tree and renames
it, as `placement::freeze_and_sync` does today.

**Free space, counted as korrid writes.** There is no 2 GiB cap for store
downloads or for the unpacked result. korrid counts the bytes it writes, both
in the download and in the extractor. Before each write it reads `fstatvfs` on
the cache's file system and checks that the space available to its own user
(`f_bavail × f_frsize`), minus the bytes of that write, stays at or above 5% of
the file system's size (`f_blocks × f_frsize`). If not, it stops, deletes the
staged download or the work folder and the archive, and fails with the
existing `NoSpace` copy ("There is not enough space for this download.").

- **Why 5%.** It is the ext4 default reserved-blocks percentage. mke2fs(8),
  e2fsprogs 1.47.3, option `-m reserved-blocks-percentage`: "This avoids
  fragmentation, and allows root-owned daemons, such as syslogd(8), to
  continue to function correctly after non-privileged processes are prevented
  from writing to the file system. The default percentage is 5%." korrid
  applies the same margin for the writers that the root reserve does not
  protect.
- **What it protects.** korrid runs as the system user `korrid`
  (`nixos-module.nix`), so `f_bavail` already excludes the root reserve. On
  an ext4 file system made with the default, about 10% of the blocks stay
  free: 5% that non-root writers can still use, and 5% for root. The default
  storage root `/var/lib/korri` and the private state root `/var/lib/korrid`
  sit on the device's root file system, so the margin keeps space for saves,
  library writes and system updates. This is inferred from the module
  defaults, not checked on a device. On a file system with no root reserve,
  such as vfat or exFAT, the 5% is the only margin.
- **Declared sizes.** A size that the server or the archive declares never
  allows a write. One early refusal uses it: if a `Content-Length` is larger
  than the space above the floor, the download fails with `NoSpace` before
  the first byte, because it can never finish. A wrong `Content-Length` only
  harms its own download. A decoder that gives more bytes than the entry
  declares is a corrupt archive, as legacy `zip.ts` treated a size mismatch.
- **Peak use.** The archive and the unpacked folder exist together until
  publication. The check measures the file system, so it covers both.
- **The extractor checks too.** The extractor runs `fstatvfs` on its work
  folder, which the sandbox allows, and exits 1 at the floor. korrid then
  reports `NoSpace`, not `Unpack`, because it reads the same floor after the
  exit.
- **Copy change.** `Failure::TooLarge` keeps one use: the 512 KiB provider
  page bound in `fetch_text`, which reaches the person as the
  `ProviderUnreachable` copy, not as its own. Its own copy, "The download is
  larger than 2 GiB.", then describes a limit that no longer exists and is
  reached by no path. Proposal: change it to "The provider sent more data
  than Korri accepts." The tests that expect `TooLarge` from a game download
  (`game_download_tests.rs` lines 122, 168 and 219) become `NoSpace` tests.

**No ratio guard.** Byte counting against the 5% floor bounds the disk use of
an archive bomb, the person can cancel it in `Unpacking`, and discovery's
traversal budget bounds its entry count and depth. A ratio guard would add a
number with no grounding.

**Entry count and depth.** The extractor uses discovery's `TraversalBudget`
default (`discovery/scanner.rs`): 100,000 entries, 10,000 folders, depth 32,
and 10,000 entries in one folder. These are not new numbers. Identification
cannot see past them, so an archive above them can never become a release.
The extractor counts entries as it reads the list and fails before it writes.

**Memory.** A 7z or rar decoder allocates a dictionary whose size the archive
sets. `sevenz-rust2` 0.24.1 passes no memory limit to its LZMA decoders
(`src/decoder.rs`, `LzmaReader::new_with_props(..., None)` and
`Lzma2Reader::new(..., None)`). Its `Archive.blocks`,
`Coder::encoder_method_id` and `Coder::properties` are public, so the
extractor reads each coder's dictionary size before it decodes. `rars` takes
the same bound through `with_rar50_dictionary_size_limit`. The extractor
refuses an archive whose dictionary exceeds `MemAvailable` in `/proc/meminfo`,
read when it starts. That is a check grounded in the device. korrid has no
`/proc/meminfo` reader today. The failure copy is `Unpack`. No `MemoryMax=`
is added to the unit, because no grounded value exists. An allocation that the
check misses ends the extractor, not korrid.

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

The outer name has two jobs:

- It passes the check before the download (a system or archive extension).
- For a plain download, it is the file's name in the ready folder. For an
  archive, it names nothing in the cache. The ready folder holds the
  archive's entries under their own paths.

The ready folder becomes
`cache/<plugin short>/<provider name>/<hex of the release key>/` with the
content inside. The rest of ADR 0016 Q1 stays.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| Unpacking is a core capability | Owner answer Q1a | ADR 0012's per-plugin `unpack` program stays for declared artifacts. |
| Formats zip, 7z, rar | Owner answer Q1b, legacy `ARTIFACT_MAGIC` | The three magic rows come back. `gz`, `tar` and `png` stay out. |
| Pure-Rust extractor in the sandbox unit | Owner answer 1 (second round), ADR 0012 unpack-program contract, `unit.rs` `unit_arguments` | korrid's own binary is the program. No C or C++ reader. |
| Unit name `korri-acquire-<plugin short>-<provider name>.service` | `unit::name`, `placement::provider_cache`, ADR 0016 artifact-key and provider-name rule | Same rule as artifacts, applied to the provider cache path. |
| Archive is transport, never a release | ADR 0005 identity rule, ADR 0013 "hashes of its main content file" | Amends ADR 0005 "Archives are not supported in the first version" for store downloads. Discovery of sources still refuses archives. |
| Identification of the work folder | `identify_with_rules`, `ContentRules`, `TraversalBudget` (discovery v2) | New caller. The default layout is appended after the system's layouts. Exactly one unit and no attention finding are required. |
| Release identity kinds | `ContentIdentity` and `LibraryIdentity` `Hash`, `Parts`, `Chd` | `record_acquired` writes all three. |
| Ready folder name | ADR 0016 row "The version segment is discovery's content hash (ADR 0005)", `ContentIdentity::value` | The release key's hex without its prefix, so a CHD gets its 40-digit SHA-1. |
| Main file for the route | discovery's owner order in `identify_files` (`m3u`, `cue`, other) | Replaces `ready_game_file`'s one-top-level-file rule. |
| Safe extraction rules, modes `0644` and `0755` | Rock N Roll `unpack.py`, legacy `safeZipPath`, `O_NOFOLLOW` staging | Moved from one plugin program into korrid. |
| Entry, folder and depth limits | discovery `TraversalBudget::default()` | Reused, not new. |
| Free-space floor of 5% | ext4 default reserved-blocks percentage, mke2fs(8) `-m`. `libc` is a direct dependency. `Failure::NoSpace` copy exists. | New check in place of the 2 GiB cap. |
| 2 GiB download cap | legacy `MAX_FALLBACK_DOWNLOAD_BYTES`, ADR 0016 | Removed for store downloads (second-round decision 2). The `TooLarge` copy changes. |
| No ratio guard | second-round decision 3 | Disk, cancel and entry bounds cover the case. |
| Dictionary memory check | `/proc/meminfo` `MemAvailable` | New. No korrid reader exists today. |
| `Unpacking` job state | Owner answer Q1e, ADR 0012 `ArtifactState::Unpacking` | New `AcquireJobState` variant. Cancellable for store jobs. |
| `FinalDownload.method`, `FinalDownload.body` | legacy `PluginHttpRequestOptions.method`, `.body` | Moved to `FinalDownload`. Coordinator decisions Q2a to Q2d. |
| Redirect after POST | legacy `fetchWithValidatedRedirects`, RFC 9110 section 15.4 | Browser rules (Q2b). |
| Outer name order | legacy `resolution.filename ?? request.fileName ?? downloadUrlBasename(...)` | N1 drops `request.fileName` (Q3a). |
| Failure copy | `acquisition/types.rs` `Failure`, code `AcquisitionFailed` | No new wire code. `Unpack`, `NotSystemFile` and `NoSpace` cover the new failures. |

## Owner questions

No open owner questions.

## Out of scope: application archives

Owner answer 5 (second round) moves these to later work. The research for
them stays here so that the later ADR starts from it.

- **Main file.** Discovery cannot know which file in an itch.io-style folder
  starts the game. Main already puts that choice in the plugin's declaration:
  ADR 0013 `file: { artifact, path }`, ADR 0016 `file: { packageFile, path }`,
  and runner `releases` hashes. Legacy did the same in plugin code: the
  PortMaster installer picked launch scripts by the `.sh` suffix
  (`legacy:product/plugins/portmaster/src/installer.ts` `isLaunchScript`).
  For a store download, nothing names the main file. Legacy
  `ArtifactAcquisitionHint.file.name` names the downloaded file, not an entry
  in it. ADR 0016 also excluded itch.io ("skip itchio"). The shape is
  unresolved until a real provider plugin needs it.
- **Executable bits.** This ADR gives no file an executable bit. NVD
  CVE-2023-49102 (NZBGet) is a remote code execution that came from unpackers
  that kept executable bits. Archives also cannot be trusted to carry them:
  the Mega Man Rock N Roll ZIP "has MS-DOS attributes and carries no Unix
  modes", so its plugin lists modes in its own `contents` file. A declaration
  for modes is unresolved.

## Later: declared artifacts

Core unpacking for ADR 0012 and ADR 0013 artifacts is not in this ADR
(second-round decision 4). FEX (squashfs and `.pkg.tar.zst`) and Proton
(`.tar.xz` and a `toolmanifest.vdf` edit) gain nothing from zip, 7z and rar.
Mega Man Rock N Roll needs executable files, which belongs to the deferred
application-archive work. A later ADR can add an explicit opt-in on the
artifact declaration. This ADR does not define it. Until then, a pinned
artifact with no `unpack` program lands as the downloaded file, unchanged.

## Considered options

- **Keep one content file from the archive (first draft).** It fits one ROM
  per archive. It cannot carry a cue with its tracks or a multi-disc set.
- **A new algorithm to find "the game file".** The owner noted there is no
  deterministic way. Discovery's rules are the existing answer for ROM and
  disc content.
- **Read archives in korrid's process.** The owner chose a sandboxed
  subprocess, so a reader bug, panic or large allocation ends only the
  extractor, away from the device key and the library writer.
- **UnRAR or libarchive for RAR.** The owner chose no C or C++ reader. UnRAR
  is also under a non-free licence.
- **Keep the archive whole and let the runner open it (legacy).** Legacy
  RetroArch discovery claimed `.zip` and `.7z` for cart systems
  (`product/plugins/retroarch/src/discovery.ts` lines 92–144). The release
  identity would then be the archive's bytes, and the same game from two
  sites would make two releases. ADR 0005 and the discovery v2 brief refuse
  this.
- **Each provider plugin ships an unpack program (ADR 0012 style).** The owner
  chose core. Provider plugins are source only today
  (`nix/product/PUBLISHED-PLUGINS.md`).
- **A ratio guard.** Rejected by second-round decision 3.
- **The plugin fetches the bytes (`artifact.acquire`).** ADR 0016 dropped this
  operation, because it needs I/O in the sandbox.

## Consequences and stated limits

- **`rars` is young.** Version 0.11.0, first release 2026-05-13, about five
  months and 30 releases old. Its README says it "started as an agentic
  development experiment" and "could use more testing at volume". A RAR file
  that it reads wrongly or not at all fails that download with the `Unpack`
  copy. A bug in it runs in the sandbox unit, not in korrid. romhustler is the
  only sampled site that sent RAR.
- **Out of scope:** application archives, declared artifacts, nested
  archives, split and multi-volume archives, encrypted archives, `gz` and
  `tar` for store downloads, multi-disc sets without an `.m3u` that no system
  layout combines, arcade sets where the zip is the content, and
  `kind: "patch"` claims.
- **Discovery is unchanged.** An archive in a source is still not a game.
- **One release per download.** An archive with several games fails.
- **Disk.** Peak use is the archive plus its contents. Each store download
  leaves at least 5% of the cache's file system free.
- **A second full read.** Identification reads every content byte after the
  extractor wrote it. For a 4 GiB ISO that is 4 GiB more reading on the
  device.
- **Signed links.** A store job resolves only when its queue turn starts
  (`game.rs`, around line 560), so coolrom's link is fresh when the download
  starts. Whether coolrom stops a transfer that runs past about 300 s is
  unknown.
- **Host pools.** WoWROMs picks its file host from a pool. A host outside the
  plugin's `hosts` fails as `UndeclaredHost` until the plugin is updated and
  approved again.
- **Content.** These sites distribute games that the person does not
  necessarily own. Korri adds no such provider. The private `@simonwjackson`
  plugins and their use are the owner's matter. Legacy
  `PluginMetadata.legalRisk` is not carried (ADR 0016).
- **Not checked on a device.** No change in this ADR has run on hardware.

## What an implementation changes

No code changes with this ADR. Line numbers are from main at `ad3da260f`.

| File | Place | Change |
|---|---|---|
| `services/korrid/src/acquisition/game.rs` | lines 603–616 | Outer name (N1) and the check before the download (system or archive extension) |
| same | `fetch_game` call, line 644 | Pass `method` and `body` |
| same | line 662 and `publish_and_record` | `Unpacking` state, extractor unit, identify the work folder, publish, record the `ContentIdentity` |
| same | `acquired_game_dirs`, `acquired_game_content` | Strip `sha256:` or `sha1:`. Main-file rule for a folder with several files. |
| `services/korrid/src/acquisition/download.rs` | `send` (line 163, `client.get` at 174) | Method, form body, redirect rule (Q2b) |
| same | `MAX_GAME_BYTES` (line 43), `game_cap`, `with_game_cap`, `fetch_game` (line 257) | Remove the cap. Free-space floor before each write and the `Content-Length` early refusal. Keep the lead bytes for magic detection. |
| `services/korrid/src/acquisition/claims.rs` | `FinalDownload` (line 187), `admit` (line 424) | `method`, `body`, the 8192-character bound, the stricter `filename` check |
| `services/korrid/src/acquisition/policy.rs` | lines 106–140 | The name rule and its comment |
| `services/korrid/src/acquisition/placement.rs` | `publish_acquired_game` (274), `ready_game_file` (310) | Publish a folder. Find the main file. |
| `services/korrid/src/acquisition/types.rs` | `AcquireJobState` (191), `Failure` (265, `TooLarge` copy at 298) | `Unpacking`. New `TooLarge` copy. |
| `services/korrid/src/acquisition/unit.rs` | `name`, `unpack`, `reclaim` | Unit name and start, stop and recovery for a provider job |
| new module under `services/korrid/src/acquisition/` | new | The extractor mode: zip, 7z and rar readers, safe extraction, free-space and memory checks |
| `services/korrid/src/main.rs`, beside `catalog_cli.rs` | new | The extractor subcommand |
| `services/korrid/src/discovery/reconcile.rs` | `AcquiredGame`, `record_acquired` (413) | Carry and write `Hash`, `Parts` or `Chd` |
| `services/korrid/src/discovery/identify/mod.rs` | `identify_with_rules` caller | Rules copy with the default layout appended |
| `services/korrid/Cargo.toml` | `[dependencies]` | `zip`, `sevenz-rust2`, `rars` with the features above |
| `services/korrid/src/acquisition/provider_tests.rs` | lines 136, 178, 774–777 | The `zip` fixture becomes archive tests: each format, magic mismatch, cue with tracks, `.m3u` set, CHD, several games, unsafe entries, free-space floor, dictionary bound, cancel in `Unpacking` |
| `services/korrid/src/acquisition/game_download_tests.rs` | lines 122, 168, 219, and new tests | `TooLarge` cases become `NoSpace` cases. POST, redirect after POST, `filename` naming. |
| `services/korrid/SCRIPTING.md` | "Request and response turns", "Results", "RPC", "A provider download stays in korrid's cache" | The new fields, state, cap removal and rules |
| `contracts/generated/korrid.ts` | whole file | Regenerated by `nix run .#korrid-check`, never by hand |
| `docs/adr/0005-release-identity.md`, `docs/adr/0012-korrid-acquires-declared-artifacts.md`, `docs/adr/0016-korrid-acquires-games-from-provider-plugins.md` | amendment notes | On acceptance: archives for store downloads, the provider unit name, the 2 GiB cap, the ready folder name, `FinalDownload` fields, `Unpacking` |
