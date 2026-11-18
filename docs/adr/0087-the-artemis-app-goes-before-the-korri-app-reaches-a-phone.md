---
status: accepted
---

# The Artemis app goes before the Korri app reaches a phone

**Owner decision, 2026-11-18 (issues #434, #391).** Korri had two Android
apps. The Artemis app (`clients/android`) was the trimmed Moonlight fork with
Korri's Java, and it was the only Android app that streamed for a person.
The Korri app (`clients/android-app`) runs korrid, the shell and the Rust
viewer in one library, with no Korri Java. The route brief
(`docs/research/2026-11-14-android-app-route.md`) kept the Artemis app until
slice D7′, and gated D7′ on live resize (#476), real rumble (D4b), every
stream control (D11), local Android games (D13) and a measurement against
the Artemis app on the owner's phone (D12, ADR 0062).

On 2026-11-18 the Korri app had passed its emulator proofs, but it had not
run on a phone, 6 of Artemis's 13 stream controls were missing, and local
Android games were not carried. The owner said:

> It can go immediately. I'm willing to do scorched earth here and then just
> iterate on the fine details until we get it right. Even if that means
> Android streaming will not be available for a while.

## Decision

- The Artemis app goes now. Its tree, its Gradle and Java, its evidence and
  proofs, korrid's JNI bridge that only it called, and its Nix outputs and
  tasks are removed. The tag `artemis-final` marks the last commit that
  carried it. `docs/briefs/2026-11-18-artemis-removal-scope.md` records what
  went and what stayed.
- The Korri app replaces it. A missing Artemis behaviour is ported into Rust
  from the tag, one slice at a time. No Java or Gradle app comes back.
- The six stream controls that the Rust viewer does not carry:

  | Artemis control | Decision |
  | --- | --- |
  | Full keyboard | Dropped. Rebuilt later, product first |
  | Keyboard as controller (the on-screen controller) | Dropped. Rebuilt later, product first |
  | Picture-in-picture | Skipped |
  | SGSR sharpness, SGSR edge threshold | Backlog, as a list of upscalers |
  | Touch sensitivity | Backlog |

  "Product first" means that the design starts from what a person needs,
  not from Artemis's view.
- For the Artemis app, this decision replaces two rules: ADR 0062's rule that
  a Java part stays until its Rust replacement measures equal or better, and
  the D7′ gates of the route brief.

## Why

The Artemis app kept a second app, a Gradle build and 732 files (39 MB) of
Java, C, evidence and tools alive beside the app that replaces it. Each viewer change had to keep
the two apps in step. The owner prefers one Android app now, and accepts
that Android streaming can be unavailable for a person until the Korri app
reaches a phone.

## Considered options

- **Keep the Artemis app until D7′'s gates pass** (the route brief's plan).
  Android streaming stays available, and D12 can measure the Rust viewer
  against the Artemis app on the same phone. Cost: two apps until D12 and
  D13 are done, and those dates are not known.
- **Remove the Artemis app now** (chosen).

## Consequences

- Until the Korri app reaches a phone (D12), no Android app streams for a
  person.
- D12 has no Artemis app to measure against. ADR 0062's numbers for Android
  can come only from the Korri app's own runs. A comparison would have to
  build the old app from the tag; nobody has tried that.
- Local Android games (ADR 0054) have no launcher until D13. korrid keeps
  its side (`android_local`), with no caller.
- The Artemis release key's certificate and the Odin 2 Portal launcher tools
  stay at the tag only.
