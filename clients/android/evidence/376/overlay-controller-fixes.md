# Manual overlay follow-up (2026-11-08)

Merged designer branch `c-afd8c04b958ce6a924a7821d66675606951f2f66d097e8f8e6355983a90bdc09` in `f9e57bfa2`. Resolved the three Pico/Zone conflicts using the designer's files, including their transparent tokens. No new layout was designed.

Guide previously delivered Back on down (closing the overlay), then Game opened it again on up. Guide/Back now capture a complete press; only its up edge acts. Guide closes through the surface's Menu semantic, so a local destructive question does not intercept it. Zone now handles that existing Menu semantic on PAUSED too. Back keeps its question-cancel behavior. Non-Korri legacy overlay behavior is unchanged.

Game forwards joystick and DPAD-source hat/stick samples through the host-neutral Surface bridge. Rust's existing AndroidInput/Router owns the 0.6 stick threshold, D-pad priority, 400 ms first repeat and 100 ms subsequent repeat. Cancel and Surface teardown clear held samples. Added the exact JNI keep rule for `shellTakeOverlayAxes()`.

Touch remains blocked from the game. Per the parent's scope decision, no pointer forwarding or surface treaty change was added: tappable controls are a product-wide follow-up.

Checks:

- Focused Android JVM tests: 27 passed (toggle, real Game key/motion dispatch, Surface lifecycle/keep rules, viewer state/boundaries).
- Android input Rust tests: 6 passed; hosted overlay render test: 1 passed.
- Rust fmt and clippy/typecheck (`--lib --test hosted_overlay -- -D warnings`): passed.
- Focused Pico/Zone behavior tests: 30 passed. Zone Menu behavior was reproduced as failing before its fix.
- Pico and Zone TypeScript typechecks: passed. Zone `logic.js` regenerated with `bun run zone:build`.
- `nix run .#android-apk`: passed, including both surface verifiers and arm64 native compilation. Final asset rebuild passed.
- Android lint still fails with **20 errors and 281 warnings**, matching the earlier counts. The first error remains `StreamSettings.java:134`, `MissingSuperCall`. No lint diagnostic names the new home-key class or changed Surface bridge classes. This establishes the same counts/first error, not an independently compared complete issue set.
- `git diff --check`: passed.

Final APK SHA256: `cd8768972eee1108e1141ee88e5cee09c17a1e3eee554d575778468c175f7715`.

The parent authorized `adb install -r` on `100.110.243.2:39705`. The user owns all phone navigation and manual controller testing after installation.
