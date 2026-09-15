# Escape hatches — retain the legacy design

**These are existing legacy rules, not a new universal extraConfig feature.**
The author can pass native configuration even when the typed vocabulary
lags behind a newer emulator. Korri must not reject a native key merely
because it is absent from an older typed setting schema.

```yaml
launch:
  overrides:
    args:
      prepend: ["--example-before"]
      replace: ["--replacement-routed-arguments"]
      append: ["--example-after"]
    config:
      prepend: |
        native_key = "value"
      append: |
        newer_native_key = "value"
      # replace: <whole native config, only where the adapter supports it>
```

The names above are structural examples, not real emulator flags/keys.
An adapter declares which forms it supports and provides a useful error for
unsupported forms. A full custom handler is the final escape hatch.

## 1. Fold across the settings cascade

From `foldLaunchOverrides` in legacy:

| Field | Fold rule |
|---|---|
| args.prepend | Concatenate less-specific then more-specific arrays |
| args.append | Concatenate less-specific then more-specific arrays |
| args.replace | Most-specific defined array wins, including [] |
| config.prepend | Join nonempty fragments, less-specific then more-specific, with newline |
| config.append | Join nonempty fragments, less-specific then more-specific, with newline |
| config.replace | Most-specific defined string wins, including empty string |

Do not apply generic array replacement to these fields. Fold and application
are separate: folding does not decide what part of a native config can be
replaced.

## 2. Apply argument overrides

Preserve the shared legacy composition:

```text
leading structural arguments
+ overrides.prepend
+ (overrides.replace OR generated routed arguments)
+ middle structural arguments
+ overrides.append
+ trailing positionals
```

`replace` replaces the routed segment, NOT the whole argv. For example,
RPCS3 keeps its structural config selection and trailing game directory.
The adapter owns those segment boundaries. An author who needs a different
command structure uses a custom prepare/compose handler.

## 3. Apply native config overrides

There is no single text-order rule for every application.

### RetroArch

Legacy renders typed assignments, then raw prepend/append assignments.
It rejects whole-file config.replace, malformed assignment keys, plaintext
credential keys, and argument overrides that reselect the core/config/log
structural paths. Additive config-file selection has its own typed field.
These adapter-specific rules remain, not a new general locking framework.

Important: legacy string order is evidence of the authored convention, not
proof that duplicate native assignments have the desired effect. The main
renderer later emitted one assignment per key because native duplicate
lookup ordering mattered. Preserve the user's override intent using the
native parser's behavior, with one effective assignment where appropriate.
Do not introduce a second conflicting precedence model or promise that
simply appending text always wins.

Raw native config is not limited to the old typed key list. Keep native
syntax and structural validation; do not turn source-derived evidence for
one version into a blanket rejection of a newer version's escape hatch.

Core options are a separate mechanism from RetroArch frontend config. A
core-options file must be selected through the correct core-options path/
interface for that build, not treated as --appendconfig.

### Ryubing

Legacy parses prepend/append as JSON objects and deep-merges them over the
generated config, append last. config.replace replaces the generated object.
Empty/non-object parsed fragments follow the legacy helper's behavior;
malformed JSON produces an error. Do not replace this with text concatenation.

### RPCS3

Legacy uses native YAML generation/merging and supports its documented
config.replace behavior. Its materializer reads the canonical config and
writes a per-release file without clobbering the operator's canonical file.
Port the adapter and tests, not a guessed generic renderer.

## 4. Environment and credentials

Keep environment overrides explicit and separate from native config text.
The adapter/host must preserve intended unset behavior. Credentials are
provided through the appropriate runtime channel, not embedded into source
or logged in a resolved-plan preview. This is basic data handling, not a
new approval system.

## 5. Grounding

Branch `legacy`, commit `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`:

- `product/platform/library/config/records/library-item.ts:118-132`
  — LaunchOverrides shape.
- `product/platform/library/config/cascade-resolver.ts:483-550`
  — foldLaunchOverrides accumulation/replacement.
- `product/platform/library/config/apply-overrides.ts:1-53`
  — segmented argv composition.
- `product/platform/library/config/apply-overrides.ts:56-101`
  — object merge and fragment parsing.
- `product/plugins/retroarch/src/launch-spec.ts:81-117,120-218`
  — frontend config ordering, argv placement and adapter guards.
- `product/plugins/retroarch/src/launch-spec.test.ts:655-837`
  — escape-hatch regression expectations (source review only).
- `product/plugins/ryubing/src/launch-spec.ts:187-209`
  — JSON override application.
- `product/plugins/rpcs3/src/materializer.ts:159-184`
  — canonical config and generated per-release file.

These source facts were checked for this consolidation. Tests have not been
run as part of this design delivery.
