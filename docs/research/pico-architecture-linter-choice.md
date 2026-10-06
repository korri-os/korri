# Which linter should enforce Pico's state architecture

**Date:** 2026-10-06
**Question:** Option A moves Pico's navigation into one model and a pure
`update`, the Elm architecture in React. Which linter can enforce those rules?
**Answer:** oxlint. It enforces all four rules with built-in rules and one
config file, and it needs no custom code. It is the only candidate whose
type-aware rule works on both TypeScript 5.9 (Pico today) and TypeScript 7.

## The four rules

| # | Rule | What it means |
|---|---|---|
| R1 | The core is pure | `update.ts`, `shown.ts`, `model.ts` and `messages.ts` import no React and use no `document`, `window`, timers, `localStorage` or `performance`. |
| R2 | One door to the host | Only `requests.ts` (`performRequest`) calls the host commands: `launchGame`, `runAction`, `runGameAction`, `changeSetting`, `dismiss`, `retry`, `reload`, `dismissSettingsProblem`. |
| R3 | No navigation state in the root | `PicoSurface.tsx` does not import `useState` or `useReducer`. |
| R4 | Every Message is handled | A `switch` over a union covers every member, or has a `default`. |

TypeScript already catches a missing case in `update` itself: a `switch` that
returns a value fails with "lacks ending return statement". R4 covers the other
switches. TypeScript accepts a statement `switch` with missing cases (verified
below), so R4 needs a linter.

## Method

I tested each linter on the code it would guard, not on its documentation. I
did not rely on the docs alone for any result in the main table.

- **Clean input:** the option A sketch from `/tmp/pico-nav-sketch/sketch`,
  type-checked against the real `surfaces/pico/src` and `contracts/`.
- **Tripwires:** a copy with one planted violation per rule: a React import, a
  `document` write and a `setTimeout` in `update.ts`; a `host.launchGame` call
  in `shown.ts`; a `useState` import in `PicoSurface.tsx`; and a statement
  `switch` over `PicoMessage` that covers 1 of 37 tags.
- **Harder cases:** an exhaustiveness check on a union reached through an
  indexed-access type (`SurfaceModel["status"]`); two ways to dodge R2 (by
  destructuring `host`, and by renaming it); and a function parameter named
  `document` that shadows the global.
- **Versions** (latest on npm on 2026-10-06): eslint 10.12.0 +
  typescript-eslint 8.71.1, oxlint 1.87.0 + oxlint-tsgolint 7.0.2003,
  @biomejs/biome 2.5.15, @ast-grep/cli 0.45.3. TypeScript 6.0 for the main
  run, then 7.0.2 for the compatibility run.

The lab is at `/tmp/linter-lab` (TS 6) and `/tmp/linter-lab-ts7`. Each has
the config file for every tool. `make-broken.sh` rebuilds the tripwires.

## Results

| | ESLint + typescript-eslint | **oxlint + tsgolint** | Biome | ast-grep |
|---|---|---|---|---|
| R1 pure core | ✅ built-in rules | ✅ built-in rules | ✅ built-in rules | ⚠️ hand-written; no scope analysis |
| R2 host door | ✅ `no-restricted-properties` | ✅ `no-restricted-properties` | ✅ GritQL plugin | ⚠️ needs one rule per language (`.ts` and `.tsx`) |
| R3 no state in root | ✅ `no-restricted-imports` + `importNames` | ✅ same | ✅ same | ✅ hand-written |
| R4 exhaustive switch | ✅ type-aware | ✅ type-aware (tsgolint) | ⚠️ nursery; uses its own type inference | ❌ no type information |
| Planted violations caught | 6 of 6 | 6 of 6 | 6 of 6 | 4 of 6 at first, 5 of 6 after the `.ts` fix |
| Union via indexed access | ✅ | ✅ | ❌ missed | ❌ |
| Dodge R2 by destructuring | ✅ caught | ✅ caught | ❌ missed | ❌ missed |
| Dodge R2 by renaming `host` | ❌ missed | ❌ missed | ❌ missed | ❌ missed |
| Parameter named `document` | ✅ no false alarm | ✅ no false alarm | not tested | ❌ 2 false alarms |
| Custom code needed | none | none | one `.grit` file | four YAML rules |
| Wall time, clean sketch | **1.9 s** | **0.25 s** | 0.21 s | 0.6 s |
| Works with TypeScript 7 | ❌ refuses to run | ✅ same 8 findings as on TS 6 | not tested | n/a |

All four tools also flagged one real problem in the clean sketch.
`PicoSurface.tsx` line 96 calls `host.runAction(id)` for the runner picker,
which skips `performRequest`. Option A must route that call through a
`PicoRequest`, or allow it by name.

### What each result rests on

- **ESLint and TypeScript 7.** With `typescript@7.0.2` installed, ESLint stops
  with `Error: typescript-eslint does not support TS 7.0.` The supported range
  is `>=4.8.4 <6.1.0`, from the typescript-eslint
  [dependency versions page](https://typescript-eslint.io/users/dependency-versions/)
  and its npm `peerDependencies`.
- **oxlint's type-aware rules.** These run in tsgolint, a Go program built on
  typescript-go. They do not use the project's installed `typescript` package.
  That is why oxlint gave the same results on the TS 6 and TS 7 copies. Its
  docs state "59 out of 61 type-aware rules from typescript-eslint"
  ([type-aware linting](https://oxc.rs/docs/guide/usage/linter/type-aware.html)).
- **oxlint's JS plugins.** These are alpha and use the ESLint v9 plugin API.
  They **cannot use type information yet**
  ([JS plugins](https://oxc.rs/docs/guide/usage/linter/js-plugins.html), "Not
  supported yet"). oxlint has no `no-restricted-syntax` rule, so a rule that
  needs an arbitrary AST selector means writing a JS plugin. The four rules
  above did not need one.
- **Biome's exhaustiveness rule.** `useExhaustiveSwitchCases` sits in the
  `nursery` group, and its diagnostic category is
  `lint/nursery/useExhaustiveSwitchCases`. It found the union imported through
  the `@contracts` path alias. It missed the same union reached through an
  indexed-access type (`Holder["m"]`, `SurfaceModel["status"]`). Biome
  restricts plugins to files with `includes`
  ([linter plugins](https://biomejs.dev/linter/plugins/)).
- **ast-grep.** It matches syntax with no scope or type information. A rule's
  `language: Tsx` does not apply to `.ts` files, which is why it missed the
  planted `.ts` violation until I added a second rule.

## Recommendation: oxlint

oxlint does all four rules with built-in rules and one `.oxlintrc.json`. It
catches the destructuring dodge. It does not raise a false alarm on a
shadowed `document`. It ran 7x faster than ESLint here. It is also the linter
Foldkit's own plugin targets, so the rules carry over if the portal ever moves
to Foldkit (option C).

### What it costs

- **A new tool.** The repo has no linter today. `pico-check`
  (`nix/tasks.nix`) would add an `oxlint` step.
- **Two packages, one not in nixpkgs.** Type-aware linting needs
  `oxlint-tsgolint`, which nixpkgs does not package. Its native binary arrives
  through `bun install`, like Pico's other dependencies. nixpkgs ships oxlint
  1.85.0; npm has 1.87.0.
- **Type-aware custom rules are out of reach.** A rule like "nothing of type
  `SurfaceHost` may be called outside `performRequest`" would catch the rename
  dodge. Only ESLint can run that rule today, and ESLint does not run on
  TypeScript 7.
- **Alpha plugin API.** Any future rule that oxlint lacks becomes a JS plugin
  on an alpha API.

### What no linter solves

- **Renaming `host`.** All four tools match the name, not the type. Code
  review must catch `const korri = host; korri.launchGame(…)`. The same holds
  for Foldkit's lint rules.
- **Intent.** A linter checks structure. It cannot see a decision made inside
  a page callback when it should be a Message.

### When ESLint is the better choice

Choose ESLint if you need a custom type-aware rule (for example, the
rename-proof host rule) and Pico stays on TypeScript 6.0 or older. Today's
`main` is on 5.9.3, so that works now. The cost is that the TypeScript 7
upgrade then waits for typescript-eslint, and its timing is not known.

## Found on the way: Pico's existing gates break on TypeScript 7

**Matters: high, if Korri upgrades to TypeScript 7.**

`decomposition-gate.test.ts` and `css-ownership-gate.test.ts` import
`typescript` and call `ts.createSourceFile`. In TypeScript 7.0.2, the package
root exports only `version` and `versionMajorMinor`. `ts.createSourceFile` is
`undefined`. The new API sits under `typescript/unstable/*`. Verified with
`require("typescript")` on 7.0.2.

So the TS 7 upgrade breaks those gates, and any new gate tests built the same
way. That is one more reason to put new architecture rules in oxlint and not
in compiler-API gate tests.

## Reference config (oxlint)

This config caught all 6 planted violations in the lab, plus the real one. The rule names and options
are the same as ESLint's.

```jsonc
{
  "plugins": ["typescript"],
  "options": { "typeAware": true },
  "rules": {
    "typescript/switch-exhaustiveness-check": ["error", { "considerDefaultExhaustiveForUnions": true }],
    "no-restricted-properties": ["error",
      { "object": "host", "property": "launchGame", "message": "Return a PicoRequest; only performRequest calls the host." }
      // … one entry per host command
    ]
  },
  "overrides": [
    { "files": ["**/requests.ts"], "rules": { "no-restricted-properties": "off" } },
    {
      "files": ["**/update.ts", "**/shown.ts", "**/model.ts", "**/messages.ts"],
      "rules": {
        "no-restricted-imports": ["error", { "paths": [{ "name": "react" }, { "name": "react-dom" }] }],
        "no-restricted-globals": ["error", "document", "window", "setTimeout", "setInterval", "localStorage", "performance"]
      }
    },
    {
      "files": ["**/PicoSurface.tsx"],
      "rules": {
        "no-restricted-imports": ["error", { "paths": [{ "name": "react", "importNames": ["useState", "useReducer"] }] }]
      }
    }
  ]
}
```

The Pico README asks that every gate be seen failing once before anyone trusts
it. The tripwire copy does that job for lint rules. A real setup should keep
it as a test fixture and assert that the linter reports each planted
violation.

## Outcome (2026-10-06)

Adopted. `surfaces/pico/.oxlintrc.json` holds the four rules, with oxlint
1.87.0 and oxlint-tsgolint 7.0.2003, and `bun run lint` runs them in
`pico-check`. `test/architecture-lint.test.ts` requires each planted violation
in `test/lint-tripwires` to be reported and nothing else.

Three things changed from the reference config above:

- Caliper parts and the fixture host may call the host. They are previews
  wired to a fake host, and the product never runs them.
- Rule 3 covers all of `src`, with an allow list of leaf files that keep local
  state, each with its reason.
- `importNames` on `react` also refuses `import * as React`, so
  `React.useState` cannot get past rule 3.

Pico's three compiler-API gates moved to oxc-parser and oxc-resolver
(`test/source-ast.ts`) when Pico moved to TypeScript 7.0.2.

Later the same day, `PicoCartShelf` and `PicoSettingsPanel` came off the allow
list: the chosen cart and the settings group became props kept in the model.
The leaves that remain hold typing in progress, secrets or measurements.

Later still, the remaining leaves joined the models too: typed text, the
keyboard's layers, the identity form and its QR code, the MENU hint, a
range's held value and the stored face. The allow list now holds only the
runtime (`use-pico-program.ts`) and Caliper parts.
