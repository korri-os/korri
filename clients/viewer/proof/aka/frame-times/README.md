# Frame record and number 1 on aka (#435, #432)

Three sessions of five turns each, on 2026-11-14, on a quiet aka: no game,
no other cadence run, a load below 1 and Tctl below 80 °C at the start. In
every run of all three sessions Tctl stayed between 61.2 and 65.0 °C, and what
else ran on aka was 0.8% to 2.4% of all CPUs.

- A, the baseline: Moonlight with #466,
  `/nix/store/38crl2m37akjqqscrmnq1gbwrp8l3rcx-moonlight-embedded-korri-2.7.1-korri`.
- B, the candidate: Korri's viewer with the frame record,
  `/nix/store/cbgy3i6mqmcrxfy3x5094x30mxjw5hgb-korri-viewer-0.1.0`
  (viewer source of commit "feat(viewer): write a frame record when the
  outside measure asks").
- Both: private pair on aka, base measure, 1920x1080 at 60 fps, `h264_vaapi`
  on the sender, `x11_vaapi` and `decode=vaapi` on the viewer.

```sh
stream-cadence view --sender root@aka --sender-fixture --viewer root@aka \
  --viewer-package A --candidate-package B --runs 5 [--frame-record] --out DIR
stream-cadence verdict DIR/baseline/results.json DIR/candidate/results.json
```

| Session | Start (UTC) | Tool | Record | Directory |
| --- | --- | --- | --- | --- |
| 1 | 08:07 | before the record became opt-in: the tool asked every viewer | written by B | `record-on-before-opt-in/` |
| 2 | 08:24 | the same tool, with `--no-frame-record` | none | `record-off/` |
| 3 | 08:44 | final tool, with `--frame-record` | written by B | `record-on/` |

All 30 runs are valid. Every run passed the sender and viewer restore
checks and the viewer isolation proof, and every session's final cleanup
passed. No product unit changed, no game unit was live, and no entry under
`/dev/input` appeared or went. Each copied recording (79.4 to 79.7 MB) was
removed from aka after the copy; no run directory was left there.

## Verdicts

| Number | Metric | Session 2, no record | Session 1, record | Session 3, record |
| --- | --- | --- | --- | --- |
| 1 | `packet_to_display_ms` | not measured | not measured (A has none) | not measured (A has none) |
| 1 | `latency_p50_ms` | equal | equal | equal |
| 1 | `latency_p95_ms` | equal | equal | equal |
| 2 | dropped, repeated, late, torn, unreadable, output repeats | equal | equal | equal |
| 3 | `cpu_seconds_per_second` | **equal** | **worse** | **worse** |
| 4 | `startup_ms` | equal | equal | equal |
| 6 | stalls, viewer exits | equal | equal | equal |

Session 3 is refused by the final tool, because only B wrote a record
(condition `frame_record`). `record-on/verdict-allowed.txt` is the same
verdict with `--allow-different-conditions`. Numbers 3 (battery) and 5 are
not measured, as before.

**What the record costs.** Korri's viewer without the record equals
Moonlight in every measured number, CPU included (A 0.01098 to 0.01169, B
0.01081 to 0.01182 CPU seconds each second). With the record, B used 0.0003
to 0.0010 CPU seconds each second more than the A turn before it, and CPU
became worse in both sessions (session 1: A 0.01129 to 0.01166, B 0.01193 to
0.01233; session 3: A 0.01081 to 0.01098, B 0.01115 to 0.01166). A
microbenchmark of the record on a tmpfs gave 8 to 9 µs CPU for each frame,
about 0.0005 CPU seconds each second at 58 frames each second; most of it is
the write of the recording. Across sessions 2 and 3 the same binary with and
without the record is `equal` in CPU (`record-cost-verdict.txt`), so the
cost shows only against the turn before it.

So the tool asks for the record only with `--frame-record`. Number 1 comes
from runs of its own (session 3). The verdict of every other number comes
from runs with no record (session 2).

## Number 1 and the check (session 3)

Number 1 is the viewer's own time from the last packet of a frame
(`enqueue_ns`) to its hand-off to the display (`display_ns`), for each ID
that the capture shows first in the scored 60 s. The check is the capture's
display time less that hand-off, by frame ID.

| Run | Matched IDs | Unreported | Before hand-off | Number 1 p50 / p95 / max (ms) | Hand-off to capture display p50 / p95 / min / max (ms) |
| --- | --- | --- | --- | --- | --- |
| 1 | 3503 | 0 | 0 | 1.627 / 2.031 / 4.473 | 8.11 / 15.28 / 0.084 / 16.12 |
| 2 | 3502 | 0 | 0 | 1.627 / 2.029 / 4.783 | 8.06 / 15.33 / 0.081 / 16.10 |
| 3 | 3502 | 0 | 0 | 1.629 / 2.024 / 4.744 | 8.14 / 15.20 / 0.084 / 16.11 |
| 4 | 3500 | 0 | 0 | 1.624 / 2.021 / 4.931 | 8.11 / 15.20 / 0.078 / 16.11 |
| 5 | 3503 | 0 | 0 | 1.632 / 2.038 / 4.322 | 8.07 / 15.28 / 0.085 / 16.12 |

Session 1 gave the same picture: number 1 p50 1.656 to 1.813 ms, hand-off to
capture display p50 8.08 to 8.20 ms, minimum 0.086 ms, no ID unreported or
before its hand-off.

- **Number 1 ends at the hand-off.** From the hand-off to the capture's
  display time is 0.08 to 16.1 ms, about one output frame of the fixture's
  headless output (16 ms), median 8.1 ms. The frame waits for the
  compositor's next output frame. A real panel adds its own scanout.
- No capture display time came before the viewer's hand-off in any of the
  about 35,000 matched IDs of the ten record runs.
- In run 1 of session 3, `korri-viewer-run-1-times.csv`, the record has
  5057 units. Four were never handed over: a newer frame replaced them.
  From the last packet to `decoded_ns` the p50 is 0.10 ms; from
  `decoded_ns` to the hand-off it is 1.52 ms. With VA-API that second part
  holds the GPU's decode, which the presenter waits for.

## After the rebase onto main `7c3c1434d`

The three sessions ran the viewer built on main `056dec9d3`. The landed
viewer is built on `7c3c1434d`, which adds main's own viewer changes, for
example the window's keys under `-viewonly`:
`/nix/store/6aj74by6l1scfzyi32inv217gmg6qqaw-korri-viewer-0.1.0`. One turn
of each viewer with that package and the tool at the tip, `--frame-record`,
20 scored seconds, 09:13 UTC: both valid, cleanup passed. B: number 1 p50
1.625 ms, hand-off to capture display p50 8.06 ms (min 0.089, max 16.03),
1168 IDs matched, none unreported or before its hand-off. That is one turn,
not a verdict.

## Not shown here

- The handheld `v4l2request` path writes no units yet; number 1 has no
  value there.
- A real panel, the queue measure, and a viewer on another device than the
  sender. The record was run only in the base measure of a private pair.
