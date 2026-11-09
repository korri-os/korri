# #376 verification

## Revisions and same-command baseline

Clean detached main merge-base: `7b9aec440e6f95a8a0dcfe3a07c0a3ea9f453bde`.
Initial branch Rust check: `6debef4d8c93b5a62c64ae81d0d785b4b9f4c9ee`.
Final runtime/native source: `d40704e26` (later changes add only tests and evidence).
The baseline Git tree stayed clean. Both used the same command in their own worktree:

```sh
nix shell nixpkgs#bubblewrap -c bwrap --bind / / --dev-bind /dev /dev --proc /proc \
  --tmpfs /run/korri-plugin-host --chdir "$PWD" nix run .#korrid-check
```

| Result | Main baseline | Branch initial | Branch final |
| --- | ---: | ---: | ---: |
| Passed | 1572 | 1583 | 1577 |
| Failed | 28 | 22 | 28 |
| Ignored | 20 | 20 | 20 |
| Failure names absent from main baseline | — | 0 | 3 |

Bwrap changes ownership/socket behavior. These are failed full checks, not a passing full gate.
The changing network/save deadline failures were investigated below; no budget was widened.

### Complete failure-list comparison

| Test | Main baseline | Branch initial | Branch final |
| --- | --- | --- | --- |
| `host::launch_diagnostics::dedicated_exit_channel_tests::dedicated_exit_inode_is_host_owned_writable_by_only_the_runtime_uid_and_hardened` | FAIL | FAIL | FAIL |
| `host::launch_diagnostics::tests::failure_tail_is_bounded_redacted_and_refuses_symlinks` | FAIL | FAIL | FAIL |
| `host::launch_diagnostics::tests::runtime_files_grant_only_the_two_exact_users_not_groups_or_other_users` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_cli_recognizes_exact_evidence_and_empty_retry_without_a_socket` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_partial_failure_keeps_failed_and_unattempted_grants_for_retry` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_reconciles_exact_certificate_after_bulk_erase_and_retries_empty` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_rejects_encoded_frame_expansion_before_any_mutation` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_rejects_wrong_socket_peer_and_sends_no_frame` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_retains_grant_after_lost_reply_or_failed_delete_then_retries_absence` | FAIL | FAIL | FAIL |
| `host::moonlight_certificate::grant_retirement_tests::grant_retirement_uses_the_existing_pass_parser_for_expired_and_old_owner_grants` | FAIL | FAIL | FAIL |
| `host::saves::tests::prelaunch_check_confirms_peer_during_slow_local_declaration` | FAIL | pass | FAIL |
| `host::saves::tests::prelaunch_check_finds_split_during_slow_local_declaration` | pass | pass | FAIL |
| `host::saves::tests::prelaunch_check_projects_release_and_runner_warnings_from_pulled_history` | FAIL | pass | FAIL |
| `host::saves::tests::reachable_peer_confirms_long_history_despite_endpointless_peer` | FAIL | pass | pass |
| `host::systemd_unit::diagnostic_file_hardening_tests::diagnostic_file_tampering_keeps_only_the_generic_unit_exit_reason` | FAIL | FAIL | FAIL |
| `host::systemd_unit::startup_diagnostic_tests::failed_unit_uses_only_its_own_bounded_redacted_stderr_and_cleans_normal_completion` | FAIL | FAIL | FAIL |
| `host::systemd_unit::viewer_units::tests::linux_viewer_bootstrap_timeout_stops_the_exact_named_sandbox_unit` | FAIL | pass | pass |
| `identity_cli::offline_tests::interruption::offline_replacement_checkpoint_child_must_finish_successfully` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::interruption::offline_replacement_checkpoint_harness_rejects_panics_before_persistence_and_after_rename` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::interruption::offline_replacement_failed_directory_sync_keeps_exact_new_binding_and_warning` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::interruption::offline_replacement_failed_rename_preserves_exact_old_binding` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::interruption::offline_replacement_final_recheck_refuses_changed_current_binding` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::interruption::offline_replacement_interrupted_post_rename_keeps_exact_new_binding` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::interruption::offline_replacement_interrupted_pre_rename_keeps_exact_old_binding` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::offline_replacement_preserves_key_and_reopens_new_owner_without_enabling_import_transfer` | FAIL | FAIL | FAIL |
| `identity_cli::offline_tests::offline_replacement_readers_observe_only_complete_old_or_new_events` | FAIL | FAIL | FAIL |
| `netplay::receiver::monitor_tests::netplay_delayed_first_connection_plays_without_freezing_the_client` | FAIL | FAIL | FAIL |
| `netplay::receiver::monitor_tests::netplay_failed_holder_status_does_not_freeze_a_connected_client` | pass | pass | FAIL |
| `netplay::receiver::monitor_tests::netplay_receiver_kill_chord_after_reconnect_stops_the_replacement` | pass | pass | FAIL |
| `saves::peer::tests::object_nonces_share_replay_guard_and_known_warning_survives_transfer_deadline` | FAIL | pass | pass |
| `saves::peer::tests::prelaunch_does_not_wait_for_a_peer_out_of_reach_and_still_checks_one_in_reach` | FAIL | pass | FAIL |

### Five isolated runs of every added failure

These ran sequentially, alternating branch and clean main, under the same bwrap mounts and pinned korrid toolchain.
Each command selected exactly one test with `cargo test --lib TEST -- --exact --nocapture`.

| Test | Branch passes | Main passes |
| --- | ---: | ---: |
| `netplay_failed_holder_status_does_not_freeze_a_connected_client` | 5/5 | 5/5 |
| `netplay_receiver_kill_chord_after_reconnect_stops_the_replacement` | 5/5 | 5/5 |
| `prelaunch_check_finds_split_during_slow_local_declaration` | 5/5 | 5/5 |

All 30 runs passed. Full-suite failures did not reproduce alone; branch failure frequency was not higher in this sample.
Every run is recorded in `added-failure-repeats.csv`; raw logs remain in `.cache/overlay-376/added-failure-repeats/`.

The earlier delayed-first-connection netplay exact-stop timeout occurred on both revisions under bwrap.
Outside bwrap the branch passed three consecutive runs; main passed the first and timed out on the second.

Without bwrap, an earlier full branch unit run had 1604 passes, one failure and 20 ignored tests.
The one failure was `config::settings::tests::player_count_is_read_without_the_plugin_registry`.
It needs an absent registry, but the workstation has `/run/korri-plugin-host/enabled-packages.json`.
No shared registry was removed or changed. A private mount-namespace attempt also hit unrelated deadline failures.

## Pico timing comparison

The single test ran back-to-back on both worktrees with the exact shell-check toolchain, release profile and test flags.
Both passed twice under default scheduling. Three more runs per revision on CPU 2 all failed under load.
`pico-budget-comparison.csv` contains all 10 sets of medians, in estimated device ms with the unchanged factor and /2 allowance.

Last consecutive CPU-2 pair:

| Update | Main | Branch |
| --- | ---: | ---: |
| Catalog arrives | 16.33 ms | 16.75 ms |
| Hero publish | 14.90 ms | 15.14 ms |
| Grid menu | 26.25 ms | 29.07 ms |
| Pause menu | 8.23 ms | 8.13 ms |
| Clock | 8.28 ms | 7.78 ms |

The engine, timing test, fixture and bundled Pico logic are unchanged from main.
Pico logic SHA256 on both is `6f0ecb3a39249bb92ba88c8b978a28188be20a9eb0243b25ef1f61d3e9c5b4cc`.
The fixture still publishes two controls; it does not include the new Android Disconnect control.
The full shell check remains timing-sensitive on this loaded workstation. No limit or calibration factor changed.

## Other checks

- Full JVM/Robolectric suite: 381 passed, zero failures.
- Korrid upstream/selected-viewer regression group: 70 passed. Overlay core-control test: passed.
- Initial five focused Android/treaty/identity tests: passed.
- Shell Android input tests: 5 passed. Shell runtime integration tests: 18 passed.
- `nix run .#android-shell-check`: passed.
- Shell fmt and desktop/webOS clippy: passed; full shell check later stopped at the Pico budget test.
- Korrid clippy: completed with 31 warnings.
- `nix run .#korrid-check -- --types-only`: regenerated both treaties.
- `nix run .#android-apk`: passed, including both real surface verifiers and arm64 native builds.
- `nix run .#android-device-test-apk`: passed for the final one-session acceptance test.
- Pre-commit secret hook ran on every commit without bypass; no leaks.

## APK and pending device proof

App: `clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk` (83 MiB).
SHA256: `d4a05a2e963f611e32921e9de67d70cf0e0893e9d3d9cfedba1d970f9500e4c3`.
Device test: `clients/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk`.

The phone owner allowed a short run with no backup. No install occurred: the Fold7 was absent from `adb devices`,
and connecting to `100.110.243.2:38683` failed. The phone was released immediately. Its installed build is unchanged;
the installed revision was not checked. No app data was read, backed up, cleared or changed.
SSH to aka failed with `Permission denied (publickey)`. The test instead checks the far exact launch through the phone's korrid session status.

The prepared script is `.cache/overlay-376/run-phone-once.sh`. From this worktree, after confirming the phone is free:

```sh
nix shell nixpkgs#android-tools -c bash .cache/overlay-376/run-phone-once.sh ADDRESS:PORT 'Breath of the Wild'
```

It installs with `-r` only. In one exact game launch it performs two Home/Resume cycles (switching Pico/Zone),
captures screenshots plus a six-second screenrecord/audio sample, opens Korri and returns, disconnects/reconnects, then ends last.
It copies only new test-created evidence, then leaves the #376 app installed and opens Korri.
An instrumentation timeout force-stops the test process without clearing app data before reopening Korri.
Expected total runtime is about 2–4 minutes when aka is ready; this is an estimate, not a device measurement.
Instrumentation is capped at five minutes; app/test installs are capped at 90/30 seconds.
New evidence goes under `.cache/overlay-376/phone/run-*`; `last-phone-run` records the exact output folder.

Pending: actual overlay/video alpha, audio continuity, input delivery on the Fold7, all action effects, screenshots and MP4.
Physical controller/USB hardware was not tested. USB gameplay sends are gated below UsbDriverService, but USB-only overlay navigation is not implemented.
The host still has immutable starter authority; federation-wide takeover after a starter disconnect is a separate, unimplemented ADR 0053 consequence.
