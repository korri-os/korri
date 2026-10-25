# Plugin settings share one nested vocabulary, merged per leaf path

**Owner decision, 2026-10-25.** Amends ADR 0028's per-key merge of runner
settings.

On 2026-10-25 each plugin named its settings its own way. Gamescope used flat
Gamescope option names, the RetroArch runner used flat `retroarch.cfg` keys,
and the legacy RetroArch, Gamescope and Moonlight policies used nested
camelCase sections with different names for the same idea (resolution, frame
rate, HDR, logging). A person reading `device.yaml` had to learn each
program's language.

## Decision

- Every plugin's settings use one vocabulary: kebab-case names, shared
  sections (`video`, `audio`, `input`, `network`, `logging`, `debug`), units at
  the end of names, `enable` inside a feature section, positive switches,
  `width` and `height` as a pair, `fps`, `fps-limit` and `refresh-rate-hz` as
  separate terms, named choices instead of integer codes, and no Korri
  invariants. `docs/briefs/2026-10-25-settings-vocabulary.md` holds the rules
  (R1 to R8) and each plugin's key list.
- Settings are nested in the file. korrid merges them one leaf at a time and
  names each leaf by its dotted path (`video.fps`). The merge order and the
  scalar values stay as ADR 0028 has them; only the key becomes a path. A
  name never contains a dot.
- A plugin still owns its schema through `settings.describe`, now as nested
  JSON Schema objects.
- RetroArch, Gamescope, Moonlight and Sunshine move to the vocabulary in one
  cut, together with the engine change. Gamescope gains `settings.describe`
  in the same cut, so its settings appear in the editor.
- `null` on a setting path clears every value that less specific places set
  for that path. The plugin never sees `null`; it sees the path as not set.
  Only a leaf can be cleared, not a whole section.
- An unknown setting or a bad value never stops a launch. korrid skips it,
  keeps it in the file, and shows a launch warning that names the path. This
  replaces Gamescope's refusal (`gamescope-setting`, `gamescope-setting-pair`).
- Moonlight's settings can be set on the Device page and on game pages.
  Moonlight is never offered as a game's local runner.

## Considered options

- **Nested objects end to end, wire included.** Rejected: the largest change
  to the contracts, the fold and the editor, for no behaviour that leaf paths
  cannot express.
- **Flat dotted keys written in the file.** Rejected: the file would not be
  nested, and readability was the reason for the change.
- **camelCase, as in the legacy policies.** Rejected: `device.yaml` would mix
  cases (ADR 0028 makes Korri keys kebab-case).
- **A staged release, Moonlight first.** Rejected by the owner in favour of
  one cut. A failure in any plugin holds back all of them.
