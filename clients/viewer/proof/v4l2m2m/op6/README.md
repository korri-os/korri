# OnePlus 6 — not installed

**No picture verdict.** The owner approved the viewer on tabs6 only on
2026-11-24. No viewer archive was uploaded, installed or enabled on op6.
This is not a pass and not a decode failure.

Read-only preflight found `qcom-venus-decoder` at `/dev/video13`, active korrid
and shell, and a maximum thermal-zone reading of 33.5 °C. `preflight.png`
shows the existing shell, not the Rust viewer. The capture temporarily woke
the output, then restored its original power state (`preflight.outputs.json`).
Stock signed `grim` was downloaded from cache.nixos.org for that capture;
no software was compiled on the phone.

The later read-only check could not reach `op6.local` or `192.168.1.186:2222`
(`postflight.txt`). No stream started, so there are no stream frame, drop,
audio, controller or stop measurements.
