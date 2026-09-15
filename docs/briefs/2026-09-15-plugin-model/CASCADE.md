# Settings cascade

**This section is the recommended rule for approval.** The exact order was
not confirmed in the conversation. It is included here so the design can be
reviewed as one complete proposal rather than another partial sketch.

Family settings are defaults. A more specific choice can override them.
There is no separate user-setting lock system.

## 1. Two axes, not one list of files

First compare what a contribution applies to, from broadest to narrowest:

| Rank | Target |
|---|---|
| 0 | Effective bundled defaults for the selected runner |
| 1 | General preferences/settings for the selected participant |
| 2 | Family |
| 3 | System |
| 4 | Runner |
| 5 | Game, including a selected contained playable |
| 6 | Release |
| 7 | This launch's explicit temporary overrides |

At the SAME target rank, compare source:

1. Device configuration.
2. Person's configuration.
3. Person's configuration for this device.

Specific target wins before source. A device's runner-specific setting can
therefore beat a person's broad family default. A person's setting for that
same runner beats the device value. Show this provenance in diagnostics.

"Device" is not an unconditional last layer. Actual hardware constraints
are checked as capabilities separately, not simulated by hidden settings.

## 2. Family and runner settings keep their scope

Shared general preferences are Korri's small vocabulary. They are translated
by the selected participant at the scope where authored. Explicit native
settings at the SAME scope win over the translated preference.

There is no universal top-level native-settings vocabulary. Native settings
remain under a participant/family ID. A game/release can refine a family or
runner without copying the whole plugin declaration.

Public configuration sections use names such as `families`, `runners`,
`systems`, `devices`, `presets`, and `profiles`. No `by-` prefix.
User-facing setting names are kebab-case. Native escape-hatch names retain
the application spelling. Internal JS method names are not user config keys.

A root `systems.gba` record is system scope. A `runners.<id>` child of a
selected game record is GAME scope with a runner selector; it does not fall
back to rank 4. Families/runner selectors inside the same target are applied
family first, runner second. General preferences are applied before both.
Only documented nesting is legal; this is not arbitrary recursive scoping.

## 3. Fold values once

- Missing key: keep the earlier value.
- Scalar: later value replaces it, including false, 0 and empty string.
- Object: merge fields recursively.
- Array: replace unless the field explicitly defines accumulation.
- Native setting null: clear the inherited assignment. Retain a tombstone
  through the fold so lower defaults do not reappear during rendering.

A renderer must explain what clearing means to its backend. Omitting a key
can restore the application default; it does not always mean "disabled".
For example, use an explicit shader-enable false if that is the requested
meaning. A native escape-hatch fragment uses native null/text semantics,
not the generic setting-clear rule.

Bundled library defaults and runner defaults are composed once during build.
Their provenance can be retained for explanation, but the renderer must not
apply a second hidden default cascade after Korri resolves settings.
The separately installed family's defaults never override this seed.

Schema validation checks the effective values before use. Unsupported keys
remain in saved configuration. Report applied, unsupported, invalid, and
native-pass-through values. General preference mappings may report values
they cannot implement; do not promise mappings such as low-latency ->
run-ahead unless the runner actually supports them.

## 4. Profiles and presets

They are reusable contributions, not automatic final layers.
A profile/preset applies at the scope where selected. Its family/system/
runner selectors retain their target meaning. Within one record:

1. Included profiles in explicit order.
2. Included presets in explicit order.
3. Inline settings on that record.

Last included fragment wins against earlier fragments; inline values win
against included fragments. A preset explicitly selected for THIS launch
has launch scope and can intentionally override a game record. A general
battery profile does not unexpectedly override a game exception.

Reject cycles and ambiguous inclusion order. Do not sort by plugin name.

`inherit: false` is not added to this new proposal yet. Legacy's truncation
is real and must be migrated explicitly if existing records use it; do not
silently reinterpret it as "drop the device layer". Clearing an individual
key and cutting an entire inheritance chain are different operations.

## 5. Escape hatches have field-specific fold rules

The generic array-replace rule does NOT apply to legacy append/prepend
fields. `launch.overrides.args/config` retains its legacy accumulation and
replacement behavior. See ESCAPE-HATCHES.md. The native adapter applies
those results after ordinary settings mapping according to its own format.

## 6. Worked conflict

For the selected mGBA runner and Drill Dozer:

| Source and target | integer-scale | Result so far |
|---|---:|---:|
| Bundled default | false | false |
| Person's RetroArch family | true | true |
| Device's mGBA runner | false | false |
| Person's mGBA runner | true | true |
| Person's game-specific mGBA settings | false | false |
| This launch only | true | true |

The last row exists only if the person explicitly supplies a launch override.
Removing a row reveals the next applicable value; it does not rewrite it.
This demonstrates precedence, not a claim that integer scaling is wrong for
any particular panel.

## 7. Example homes

See `examples/settings.yaml`. It contains separate YAML documents for device,
person, and library homes. It is not one YAML file with duplicate top-level
keys. The final implementation must use a duplicate-key-rejecting loader.
