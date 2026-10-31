# Rejected exploratory runs

None of these files is the final baseline. Final GAMES samples are
`../chromium-games-{1,2,3}.csv`, each with an end screenshot and PID/restart
counter before and after. Final attract samples are `../chromium-attract.csv`.

- `chromium-drift-rejected`: the original sleep-then-read loop accumulated
  smaps overhead, taking 83.85 s for 60 samples. The fixed method schedules
  against monotonic uptime.
- `chromium-attract-rejected`: an early, pre-reboot attract capture. The final
  run has stable PID/restart checks and screenshots at both ends.
- `zone-spike-games-window-*`: the first short-window attempt. Window 1 ended
  on Loading, window 3 on attract, and Chromium had restarted during this
  period. These windows cannot be combined into a stationary GAMES baseline.
- Other screenshots record keyboard-navigation exploration and the catalog
  error dialog. No look setting was changed.

The device's Chromium repeatedly exited with SIGSEGV during exploration
(13 automatic restarts were observed). coredumpctl recorded SIGSEGV with no
core file. No OOM event was observed; the cause is unknown. Removing the runtime surface override alone
did not immediately stabilize it. One reboot restored it. The final native
proof and final GAMES/attract measurements ran after this reboot. All three
final GAMES windows and the final attract capture retained PID 2510 and
NRestarts=0. The crash cause was not diagnosed in this spike.
