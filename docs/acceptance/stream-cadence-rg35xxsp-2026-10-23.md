# SP acceptance evidence — 2026-10-23

The original measurements fail the default strict unreadable limit of zero.
Both saved recordings now also have an **offline rescore with an explicit
unreadable limit of 200**. Those rescored runs are valid under that allowance.
Unreadable frames still supply no received IDs. The bar was not shrunk, no
plugin code was changed, and the rescore made no device connections.

## Device and one manual prerequisite

Verified device: Anbernic RG35XX SP, active mode 640x480 at 60 Hz, transform
`normal`. Sunshine reports `h264_v4l2m2m` and KMS capture. These are observations,
not constants in the tool. The plugin status does not expose a version field;
the record retains its exact package path and Sunshine's reported version.

The original boot instance, PID 10164, had no working startup encoder. The tool
refused it before pairing or creating a source folder. Its full refusal record
is also retained in `nix/stream-cadence/tests/fixtures/sp/encoder-refusal.json`.

The authorized **single manual restart** followed compositor seat activity and
a confirmed powered-on output. It occurred at 2026-10-23 03:28:56 UTC, producing
Sunshine PID **13379**. A fresh `Found H.264 encoder` line followed at 03:29:03.
`nix/stream-cadence/tests/fixtures/sp/manual-restart.json` records this operation. The tool itself
never restarted Sunshine. No second restart occurred. No Pro access, firewall
change, service configuration change or publication occurred.

## Full recordings

Both commands used only `--device` and `--stream-host`, so they used the default
90 scored seconds, 10-second warm-up, 10-second windows and three-run request.
With the original strict zero-unreadable limit, each batch stopped after its
first invalid run. These are not three successful repeat measurements.

| Observation | Installed run | Panel-off run |
| --- | ---: | ---: |
| Initial output power | on | off |
| After seat activity | on | on |
| Decoded / receive frames | 5043 / 5043 | 4836 / 4836 |
| Scored receive frames | 4328 | 4130 |
| Scored torn frames | 1930 (44.59%) | 1858 (44.99%) |
| Scored unreadable frames | 95 | 130 |
| Decoder warnings / errors | 0 / 0 | 0 / 0 |
| Restore check | pass | pass |
| Device run-folder bytes before removal | 1,593,517 | 1,586,328 |
| Final retained host-file bytes | 84,533,110 | 81,246,185 |

The client explicitly requests **10000 kbps**. Sunshine reports **7308000 bps**
for the session after its native FEC/audio/overhead adjustments. Pacing is
`unmarked`, which is allowed unless vblank lock is required.

Full result records, including every torn pair, per-window counters, latency,
discovered facts and cleanup checks:

- `/tmp/stream-cadence-fp2zrq7s/run-1/result.json`
- `/tmp/stream-cadence-ea8xfy6z/run-1/result.json`

These full JSON files are evidence, not abbreviated summaries. The encoded
recordings, receive CSVs, presentation logs, fresh journal and thread samples
remain alongside them. The host paths are temporary; copy them before clearing
`/tmp` if the evidence must outlive this host session.

Example of the observed tear: one decoded frame has upper ID **209** and lower
ID **211**, each with a valid independent CRC. This is TORN, not UNREADABLE.
Separate frames contain a CRC-broken row. Those frames remain UNREADABLE.
A read-only audit at multiple scanlines also found CRC failures; the failure
cannot be excused as only a complementary-row disagreement.

## Controller and cleanup checks

The first client attempt stopped early because Moonlight 2.7.1 uses a
process-relative receive-clock epoch. The controller compared that epoch with
host uptime and sent SIGINT early. The device journal confirms `CLIENT
DISCONNECTED`, not a Sunshine exit. The shim now bridges the native receive
clock to host MONOTONIC without replacing receive time by callback time. A C
regression test covers this. Subsequent full recordings cover the complete
warm-up and scoring interval.

A subprocess test also reproduced cancellation while a sample reply was pending:
cleanup wrongly consumed that reply as its restoration result. Correlated
request IDs fix this. The real SP signal checks passed:

| Controller signal | Exit | Restore | Certificate revoked | Run folder removed |
| --- | ---: | --- | --- | --- |
| SIGINT | -2 | pass | yes | yes |
| SIGTERM | 143 | pass | yes | yes |

Evidence: `/tmp/stream-cadence-sp-signal-checks/checks.json`.

`--require-vblank-lock` stopped at the first received frame because the published
plugin has no lock marker. Its record is
`/tmp/stream-cadence-sp-require-lock/run-1/result.json`.

An independent read-only check after all device work read the persisted native
certificate state. It found **zero matching test grants**, retained the same
Sunshine identity, and confirmed PIDs **13379 / 1021 / 11533** for Sunshine,
compositor and korrid. The last run folder was absent. Evidence:
`/tmp/stream-cadence-sp-independent-clean.json`.

The first provision created Sunshine's previously absent identity state. That
allowed persistent identity file was retained, not deleted. Only the test
certificate grant was revoked. The client private key remains on this host,
not in the repository or on the device.

## Offline rescore with an unreadable allowance of 200

The original recordings and records were left unchanged. These commands decode
and score the saved video, receive timestamps and presentation feedback without
SSH or a new device run:

```sh
./nix/stream-cadence/tests/rescore.py /tmp/stream-cadence-fp2zrq7s/run-1/result.json \
  --max-unreadable-frames 200 --out /tmp/stream-cadence-fp2zrq7s-rescored-200.json
./nix/stream-cadence/tests/rescore.py /tmp/stream-cadence-ea8xfy6z/run-1/result.json \
  --max-unreadable-frames 200 --out /tmp/stream-cadence-ea8xfy6z-rescored-200.json
```

Each row below is a complete 10-second scored window after the 10-second warm-up.
Unreadable frames supply no IDs at all. A presented ID seen only in such a frame
remains a capture miss, even under the allowance. Both valid rows of a torn
frame contribute their distinct IDs. Counts are not normalized to 600.

### Installed recording: `fp2zrq7s`

| Window | New frames | Capture misses | Source misses | Torn | Unreadable |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 186 | 415 | 0 | 220 | 7 |
| 2 | 174 | 427 | 0 | 200 | 9 |
| 3 | 182 | 417 | 0 | 201 | 14 |
| 4 | 171 | 428 | 0 | 191 | 12 |
| 5 | 189 | 412 | 0 | 239 | 15 |
| 6 | 191 | 410 | 0 | 251 | 8 |
| 7 | 184 | 413 | 2 | 225 | 19 |
| 8 | 176 | 425 | 0 | 194 | 0 |
| 9 | 177 | 422 | 0 | 209 | 11 |
| Total | 1630 | 3769 | 2 | 1930 | 95 |

Valid under the 200-frame allowance: **18.111 readable new IDs/s**.
Torn fraction: **1930/4328 = 44.59%** of scored receive frames.

### Panel-off recording: `ea8xfy6z`

| Window | New frames | Capture misses | Source misses | Torn | Unreadable |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 187 | 414 | 0 | 241 | 17 |
| 2 | 173 | 427 | 0 | 186 | 11 |
| 3 | 173 | 425 | 0 | 208 | 23 |
| 4 | 181 | 420 | 0 | 221 | 23 |
| 5 | 177 | 423 | 1 | 220 | 4 |
| 6 | 187 | 412 | 0 | 229 | 8 |
| 7 | 171 | 431 | 0 | 212 | 25 |
| 8 | 191 | 409 | 0 | 227 | 4 |
| 9 | 105 | 494 | 0 | 114 | 15 |
| Total | 1545 | 3855 | 1 | 1858 | 130 |

Valid under the 200-frame allowance: **17.167 readable new IDs/s**.
Torn fraction: **1858/4130 = 44.99%** of scored receive frames.

## Remaining acceptance gap

These are two offline rescored recordings, not a new three-run device batch.
The default limit remains zero, under which both recordings are invalid. The
nonzero allowance accepts the unreadable count, not the unknown IDs or complete
frame coherence. Do not report the expected approximately 13.5 new fps as
measured. The visible capture defects still need their own investigation.
