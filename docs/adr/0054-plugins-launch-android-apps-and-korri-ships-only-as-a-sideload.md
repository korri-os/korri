---
status: proposed
---

# Plugins launch Android apps, and the Android app ships only as a sideload

**Owner decisions, 2026-11-08.** The design interview about Android apps
launched through plugins is not finished. This record holds the decisions made
so far. It becomes accepted when the interview ends.

## Decision

- **Three kinds of Android app.** A plugin may give Korri a route through
  three kinds of app: an external emulator app started with a game file
  (Dolphin, NetherSX2, Azahar, DuckStation), a native Android game installed
  on the phone (legacy `@korri:android-app`), and an emulator APK that Korri
  builds and patches itself (legacy `plugins/retroarch/android/`). The in-app
  libretro cores of #293 are a separate route and not part of this decision.
- **Sideload only.** The Android app does not ship on Google Play. This
  answers question 1 of #291. Play rules limit `QUERY_ALL_PACKAGES`,
  accessibility services, All-files access and downloaded executable code,
  and each of the three kinds needs at least one of them.
- **One plugin per program.** A program that runs on Linux and on Android is
  one plugin, with a runner for each platform. Systems, discovery and settings
  are declared once. This follows ADR 0052, where one ID carries a different
  file set for each architecture.
- **Plugins reach Android korrid through the catalog.** The korrid inside the
  Android app installs plugins from the same Korri plugin repository as Linux
  (ADR 0043), with the same signature and permission checks. A runner that
  only starts an installed app ships no native files, so it uses ADR 0047's
  "any architecture" release.

- **Native games first.** The first slice launches native Android games. A
  native game needs no game file: korrid names the package, and the app finds
  that package's current launcher activity at launch, as legacy
  `launcher/android_app.rs` did.
- **Native games in the library.** Apps that Android marks as games (`isGame`,
  `ApplicationInfo.CATEGORY_GAME`) appear on their own. The person can add any
  other installed app.
- **A missing app is shown, not hidden.** When a route needs an app that is
  not installed, the route is shown as unavailable, with the reason and a
  link to where to get the app.
- **Korri installs its own APKs.** A Korri-built APK is a pinned plugin
  artifact (ADR 0012, ADR 0052). Korri installs and updates it through
  `PackageInstaller`, and Android asks the person to confirm each time.

- **End closes the session, not always the app.** Android 14 and later let an
  app stop only its own processes, and no standalone emulator takes a quit
  command from another app. End closes Korri's live session and brings Korri
  forward. The app may keep running, and the overlay says so. A runner that
  can stop its app (RetroArch's `QUIT` network command) stops it.
- **The Guide button through an accessibility service.** A window over
  another app cannot see gamepad buttons while that app has focus. A Korri
  accessibility service with `FLAG_REQUEST_FILTER_KEY_EVENTS` catches the
  Guide button only and opens the gameplay overlay (ADR 0053). Play forbids
  this use, which the sideload decision allows.
- **Usage events tell Korri that the person left.** While a live session is
  open, Korri reads `UsageStatsManager` foreground events about once a second.
  It notices when the person switches away and when the app closes.

## Consequences

- korrid on Android does not load plugins today. Plugin installation, its
  signature checks and the plugin host must be ported before any of these
  routes can run.
- The catalog `platform` field and runner admission must name Android. The
  spelling is not decided. ADR 0047 left the spelling of "any" open as well.
- After developer verification starts in 2027, each install needs a verified
  developer account, a 24-hour wait, or ADB (#291).
- A Korri-built emulator APK is native code that Korri publishes. ADR 0042
  limits `@korri` to content licensed for redistribution, so each such APK
  needs a licence check before Korri ships it.
- Where the person's added apps are stored and how the "get the app" link is
  declared are schema questions. Each must be grounded in legacy or an
  existing producer before it is built.
- The person grants two special permissions in Android settings: Usage
  access and the accessibility service. Android shows a strong warning for the
  accessibility service. Only one service on a device can filter key events,
  and it sees buttons but not sticks.
- A game that the person ended can still run in the background. Coming back
  to it later starts a new live session.
- Korri owns a signing key for each APK it builds. Losing the key breaks
  updates for every install.
