# Play, library, downloads and launch settings

All effects use the local korrid person socket. `--device` selects the device
performing the operation, not a streaming route. Peer authorization is unchanged.
A refused remote operation needs SSH or local access, not another endpoint.

## Commands

- `korri play <game> [--runner <exact-runner-id>] [--set key=value]`
- `korri leave`, `korri resume`, `korri end --yes`
- `korri library folders list|rescan`
- `korri library folders add <path> <system>`
- `korri library folders remove <folder-id> --yes`
- `korri library games list`
- `korri library attention list|assign <item> <system>|ignore <item> --yes`
- `korri get <game> [--provider <provider-id>] [--id <claim-id>]`
- `korri get search <query> [--provider <provider-id>]`
- `korri downloads status [--game <game> | --job <provider-job-id>]`
- `korri downloads cancel --artifact <artifact-id> | --job <provider-job-id>`
- `korri downloads remove <game> --yes`
- `korri launch-settings show`
- `korri launch-settings set key=value`
- `korri launch-settings runner <runner-id>`
- `korri launch-settings set <key> --inherit`
- `korri launch-settings runner --inherit`
- `korri launch-settings profiles list|new <title>|delete <id>|use <id> --game <game>`
- `korri launch-settings profiles use --none --game <game>`

Titles match without case; exact game IDs take precedence. Multiple matches
require an explicit selection. Pipes and JSON mode list the matches and stop.
`--runner` on play selects an existing observed route; it does not persist a
runner choice. A successful play response states acceptance, not readiness.

Play checks saves before launch. In-use, release and offline warnings are
reported but do not block play. A concrete save split directs the person to
choose a snapshot through `korri saves`. An unsuccessful save observation does
not prevent the launch request. Both responses are retained, and the partial
failure causes a nonzero exit even when the launch was accepted.

Leave, resume and end observe one launch ID and control only that ID. They never
retry a stale control against a replacement session. End can report pending
teardown; it does not claim the session has ended until korrid says so.

Folder removal unregisters the library folder and never deletes game files.
Folder addition sends the selected path to `/folder-selection` on the protected
local person socket. The endpoint uses discovery's shared grant store; the CLI
then sends the one-use receipt to `LibraryFolderAdd`. The token is not printed.
The browser, control and peer routers do not install the selection endpoint.
Remote folder addition is not supported; use SSH or local access (#227).

## Settings and current producer limits

`--system`, `--game`, `--release` and `--profile` choose an existing launch
layer; Device is the default. Release and profile require their game context.
`--runner` and `--family` choose a runner scope; Any is the default.

`--system <system-id>` does not require `--game`. Without game context, the
snapshot exposes the System places already accepted by the write producer:
configured systems and systems referenced by library releases. It does not pick
a representative game or create an unknown system. System remains the write
layer when optional `--game` supplies resolution context. The snapshot must
expose the exact System and runner scope; runner scopes still depend on the
producer's available registry. Similarly, only the release exposed by the
existing resolved snapshot can be edited.

Show includes effective values and origins, authored places, plugin schemas and
read-only launch facts. Its effective values describe the producer's currently
resolved route and selected profile. Layer/scope flags inspect an authored place;
they do not request hypothetical resolution for an unselected runner/profile.
Without `--game`, effective values use Device context, including when inspecting
a System authored place. The output labels that context; the authored System
body is included separately in the unchanged snapshot. Writes pass the observed
device or games revision once.
A conflict is final; read the settings again before deciding to retry.
`--inherit` clears the authored override using the existing optional write
value. It cannot be combined with an explicit assigned value or runner.
Literal strings such as `inherit`, `null` and empty strings remain values.
Write replies contain revisions only. The acknowledgement names the place and
scope and gives a matching show command; it never invents an inherited winner.

Editable keys follow `LaunchPath`: CPU/GPU minimum and maximum percentages,
Device/Any CPU governor when advertised, runner setting names,
`<modifier-id>.enabled` and `<modifier-id>.<setting-name>`. Plugin schemas supply
setting types and choices. A key shared by a runner and modifier is refused.
Hooks, environment, arguments, working directory and raw config remain read-only.
One-launch values use `LaunchOverride`; they do not write launch settings.

A library game is downloaded through the existing game download operation.
Otherwise get selects a provider, searches, obtains details, and acquires that
same provider claim. Explicit `--provider` skips library matching. `--id` names
the provider claim, not a game or job; details still run before acquisition.

Provider jobs and declared artifact downloads have separate IDs and states.
Status without `--job` lists declared artifacts, not provider jobs. There is no
current producer for listing all provider jobs or deleting their job records.
Keep the job ID returned by get. Remove operates on a game's downloaded content,
not a job record or saves. Status displays the producer's current job state;
it does not infer completion from a request being accepted.

Every `--json` response uses the original typed RPC response. Play is an
orchestrated operation: it always returns an array, with the save-check response
and final launch response when that stage is reached. A failed earlier preflight
is a one-item response array. Other final operations return one response object.
Failed job states, failed artifacts and partial game lists remain their original
Ok payloads but cause exit 1. No domain or persistence schema is duplicated.
