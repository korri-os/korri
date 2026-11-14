# Korri's viewer on `main` against Moonlight, all measured numbers (2026-11-14)

The coordinator ran the viewer as it is on `main` at `6dfa146ff` against
Moonlight with #466, in turns on aka. Both measures were kept on their first
try. That is the first run with the number 5 measure and the #467 fix, so
startup carries no false 10 s starts.

- A: `@korri:moonlight` release `build-2094c1e77471-moonlight-latest-frame`,
  `/nix/store/38crl2m37akjqqscrmnq1gbwrp8l3rcx-moonlight-embedded-korri-2.7.1-korri`.
- B: `korri-viewer` built from `main`,
  `/nix/store/xg4vfy7wq8nsvk3qgz6174avpzp5n10i-korri-viewer-0.1.0`.
- The tool is stream-cadence at `6dfa146ff`. There were five turns each.
  Each measure started only with no game or other test unit live, load below
  1.5 and Tctl below 78 °C. Both sides decoded with VA-API.

| Measure | Other load, A / B | Verdict |
| --- | --- | --- |
| Base | 0.9–2.3% / 0.9–2.1% | equal on every measured row |
| Queue | 2.2–2.4% / 2.0–2.3% | equal on every row except `input_ms`: **better** |

The base measure, with A and B:

| Row | A (Moonlight) | B (Korri's viewer) |
| --- | --- | --- |
| Delay p50 | 11.9–12.4 ms | 11.9–12.3 ms |
| CPU | 0.010–0.012 | 0.011 |
| Startup | 737–785 ms | 768–785 ms |
| `input_ms` | 0.054–0.058 | 0.049–0.058 |
| `input_missed` | 0 | 0 |

In the queue measure, where frames arrive faster than the display shows them,
a key took 20.2–25.6 ms to leave Moonlight and 6.4–10.4 ms to leave Korri's
viewer. The cause is not known. Moonlight's X11 event loop and its
presentation share one thread, which may hold window events while frames
queue (inferred, not checked).

Numbers not measured here: packet arrival to display (Korri's viewer's frame
record is not landed yet) and battery.

Results and the unaltered verdicts are in `base/` and `queue/`.
