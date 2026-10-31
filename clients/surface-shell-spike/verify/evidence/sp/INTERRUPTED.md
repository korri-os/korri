# SP measurement validity — 2026-10-31

The SP game slice was absent at preflight. The CLI used nice 10 and four
verification workers. This run made no service stop/start calls.

All 19 completed rows in `many-*.jsonl` finished before **16:11 UTC** and are
clean. Full verification completed for N=10, 100 and 1,000 with zero failures.
All four scan/hash methods also completed for N=5,000.

Another session began repeated kiosk stop/start cycles at 16:11 UTC. The
supervisor confirmed that its independent workload was using SP and instructed
this run to stop. The partial N=5,000 full verification crossed that boundary.
It is **contended, incomplete and invalid**. `many-5000.log` is a diagnostic
progress log, not a valid full-verification time. No completed JSON result was
emitted for that phase.

Only this run's own verifier processes and its launching SSH command were
terminated. The pending large-list run was cancelled before it started. The
run's `/tmp/surface-verify-1793458262-502354` was removed. `cleanup.txt` verifies
that directory was absent and no surface-verify process remained. The kiosk
was not restarted: its inactive state belonged to the other session. The
compositor and InputPlumber retained their original PIDs.

## Explicit extrapolation, not a measurement

Use the largest clean full-verification row, N=1,000:

    962.019037748 seconds / 1,000 = 0.962019037748 seconds per item
    0.962019037748 * 5,000 = 4,810.095188740 seconds = 80.1683 minutes

The N=5,000 full-verification table row is this **extrapolation**. Its RSS and
pass/fail result are not known. No contended data contributes to that estimate.

The SP large-game-list QuickJS tests (both surfaces, 1k/10k/50k games) were
**not run**. The final prebuilt aarch64 binary supports those commands, but the
SP was not free for a valid run. Do not mix future data from that workload into
these clean rows without recording a separate run.
