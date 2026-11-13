# PC picture-path acceptance — 2026-11-12

These are acceptance runs, **not a quiet performance baseline**. One try was
made per five-run set. Before each set the operator waited up to ten minutes
for no active game, then started the set regardless. Base A exhausted that
wait; base B and queue started at a moment with no game.

Production code tested: `2336491d0`. App:
`/nix/store/vrjk65b2nnvcm1pgky6x1m8bfidwmgz4-stream-cadence/bin/stream-cadence`.
The requested Nix build passed **281 tests**, Ruff, and Mypy for all eleven
production modules. Test derivation:
`/nix/store/2bbxpjhjhp1zmipq9zy7knxx5bzw0n74-stream-cadence-tests.drv`.
The count was read with `nix log ... | grep "^Ran "`:

```text
Ran 281 tests in 43.314s
```

## Commands and results

All commands used the supplied copied product package. All sixteen runs are
valid. Each recorded `requested_platform=x11_vaapi`, `decode=software`, and
`render_nodes=["/dev/dri/renderD129"]`. Every measured viewer log reported
the failed VA display followed by `Using FFmpeg decoder: h264`. The sender
used `h264_vaapi` on `renderD128`. **VA-API decode did not run on the RX 7900
XT**; this is today's product fallback, tracked by [#439](https://github.com/korri-os/korri/issues/439).

Delay below is the median of the scored windows' p50 values, as used by
`verdict`. CPU is CPU seconds per second. `Other` is `machine.other` as a
percentage of all CPUs, not a count of cores. Queue drops include the sender's
floor and are not judged as viewer drops.

### Short run

```sh
/nix/store/vrjk65b2nnvcm1pgky6x1m8bfidwmgz4-stream-cadence/bin/stream-cadence view --sender root@aka --sender-fixture --viewer root@aka --viewer-package /nix/store/ycag8ysjam9a02nxjyvfyazqck9vg7vk-moonlight-embedded-korri-2.7.1-korri --out /tmp/cadence-vaapi-proof/short-fixed --runs 1 --seconds 10 --window 5 --warmup 2 --startup 5
```

| Run | Delay p50 ms | Drops | CPU s/s | Startup ms | Other sender % | Other viewer % | Beside game |
|---|---:|---:|---:|---:|---:|---:|---|
| run-1 | 14.629 | 0 | 0.204750 | 770.588 | 34.388 | 34.397 | yes |

Result lines:

```text
run-1 VALID: new_fps=58.200 capture_misses=0 source_misses=0 torn_frames=0/619 (0.00%) unreadable_frames=0 max_unreadable_frames=1214
run-1 viewer: startup_ms=770.6 cpu=0.2047501621070218 stalls=0 capture_gaps=0/1214 output_repeats=0
```

### Base A

```sh
/nix/store/vrjk65b2nnvcm1pgky6x1m8bfidwmgz4-stream-cadence/bin/stream-cadence view --sender root@aka --sender-fixture --viewer root@aka --viewer-package /nix/store/ycag8ysjam9a02nxjyvfyazqck9vg7vk-moonlight-embedded-korri-2.7.1-korri --out /tmp/cadence-vaapi-proof/base-a --runs 5
```

| Run | Delay p50 ms | Drops | CPU s/s | Startup ms | Other sender % | Other viewer % | Beside game |
|---|---:|---:|---:|---:|---:|---:|---|
| run-1 | 14.708 | 0 | 0.204655 | 707.084 | 34.645 | 34.645 | yes |
| run-2 | 14.853 | 0 | 0.203982 | 706.154 | 34.653 | 34.651 | yes |
| run-3 | 14.676 | 0 | 0.202141 | 754.211 | 34.596 | 34.595 | yes |
| run-4 | 14.854 | 0 | 0.205373 | 819.139 | 34.837 | 34.841 | yes |
| run-5 | 14.907 | 0 | 0.201572 | 689.592 | 32.763 | 32.758 | yes |

Result lines:

```text
run-1 VALID: new_fps=58.117 capture_misses=0 source_misses=0 torn_frames=0/3713 (0.00%) unreadable_frames=0 max_unreadable_frames=5428
run-1 viewer: startup_ms=707.1 cpu=0.20465543909319328 stalls=0 capture_gaps=0/5428 output_repeats=0
run-2 VALID: new_fps=58.117 capture_misses=0 source_misses=0 torn_frames=0/3713 (0.00%) unreadable_frames=0 max_unreadable_frames=5429
run-2 viewer: startup_ms=706.2 cpu=0.2039821281021376 stalls=0 capture_gaps=0/5429 output_repeats=0
run-3 VALID: new_fps=58.100 capture_misses=0 source_misses=0 torn_frames=0/3713 (0.00%) unreadable_frames=0 max_unreadable_frames=5411
run-3 viewer: startup_ms=754.2 cpu=0.2021414080912641 stalls=0 capture_gaps=0/5411 output_repeats=0
run-4 VALID: new_fps=58.117 capture_misses=0 source_misses=0 torn_frames=0/3712 (0.00%) unreadable_frames=0 max_unreadable_frames=5451
run-4 viewer: startup_ms=819.1 cpu=0.2053729729715947 stalls=0 capture_gaps=0/5451 output_repeats=0
run-5 VALID: new_fps=58.133 capture_misses=0 source_misses=0 torn_frames=0/3714 (0.00%) unreadable_frames=0 max_unreadable_frames=5448
run-5 viewer: startup_ms=689.6 cpu=0.20157212402812372 stalls=0 capture_gaps=0/5448 output_repeats=0
```

### Base B

```sh
/nix/store/vrjk65b2nnvcm1pgky6x1m8bfidwmgz4-stream-cadence/bin/stream-cadence view --sender root@aka --sender-fixture --viewer root@aka --viewer-package /nix/store/ycag8ysjam9a02nxjyvfyazqck9vg7vk-moonlight-embedded-korri-2.7.1-korri --out /tmp/cadence-vaapi-proof/base-b --runs 5
```

| Run | Delay p50 ms | Drops | CPU s/s | Startup ms | Other sender % | Other viewer % | Beside game |
|---|---:|---:|---:|---:|---:|---:|---|
| run-1 | 11.803 | 0 | 0.157917 | 672.610 | 0.880 | 0.880 | no |
| run-2 | 11.882 | 0 | 0.158088 | 657.303 | 0.796 | 0.794 | no |
| run-3 | 11.802 | 0 | 0.159473 | 689.253 | 0.908 | 0.909 | no |
| run-4 | 11.873 | 0 | 0.159752 | 769.861 | 1.173 | 1.172 | no |
| run-5 | 11.800 | 0 | 0.155907 | 752.974 | 0.869 | 0.869 | no |

Result lines:

```text
run-1 VALID: new_fps=58.317 capture_misses=0 source_misses=0 torn_frames=0/3735 (0.00%) unreadable_frames=0 max_unreadable_frames=5450
run-1 viewer: startup_ms=672.6 cpu=0.15791686164135887 stalls=0 capture_gaps=0/5450 output_repeats=0
run-2 VALID: new_fps=58.400 capture_misses=0 source_misses=0 torn_frames=0/3735 (0.00%) unreadable_frames=0 max_unreadable_frames=5461
run-2 viewer: startup_ms=657.3 cpu=0.15808786709945072 stalls=0 capture_gaps=0/5461 output_repeats=0
run-3 VALID: new_fps=58.400 capture_misses=0 source_misses=0 torn_frames=0/3735 (0.00%) unreadable_frames=0 max_unreadable_frames=5448
run-3 viewer: startup_ms=689.3 cpu=0.15947273483314414 stalls=0 capture_gaps=0/5448 output_repeats=0
run-4 VALID: new_fps=58.350 capture_misses=0 source_misses=0 torn_frames=0/3735 (0.00%) unreadable_frames=0 max_unreadable_frames=5454
run-4 viewer: startup_ms=769.9 cpu=0.15975168321736596 stalls=0 capture_gaps=0/5454 output_repeats=0
run-5 VALID: new_fps=58.350 capture_misses=0 source_misses=0 torn_frames=0/3735 (0.00%) unreadable_frames=0 max_unreadable_frames=5447
run-5 viewer: startup_ms=753.0 cpu=0.15590659685685856 stalls=0 capture_gaps=0/5447 output_repeats=0
```

### Queue

```sh
/nix/store/vrjk65b2nnvcm1pgky6x1m8bfidwmgz4-stream-cadence/bin/stream-cadence view --sender root@aka --sender-fixture --viewer root@aka --viewer-package /nix/store/ycag8ysjam9a02nxjyvfyazqck9vg7vk-moonlight-embedded-korri-2.7.1-korri --out /tmp/cadence-vaapi-proof/queue --runs 5 --queue-measure
```

| Run | Delay p50 ms | Drops | CPU s/s | Startup ms | Other sender % | Other viewer % | Beside game |
|---|---:|---:|---:|---:|---:|---:|---|
| run-1 | 56.583 | 232 | 0.172431 | 673.510 | 0.940 | 0.941 | no |
| run-2 | 56.799 | 231 | 0.189823 | 737.652 | 13.369 | 13.364 | no |
| run-3 | 56.604 | 237 | 0.174336 | 719.824 | 1.320 | 1.320 | no |
| run-4 | 56.557 | 229 | 0.170436 | 721.698 | 1.021 | 1.022 | yes |
| run-5 | 57.195 | 234 | 0.227559 | 689.708 | 18.719 | 18.717 | yes |

Result lines:

```text
run-1 VALID: new_fps=62.200 capture_misses=232 source_misses=0 torn_frames=0/3732 (0.00%) unreadable_frames=0 max_unreadable_frames=5453
run-1 viewer: startup_ms=673.5 cpu=0.17243083159470207 stalls=0 capture_gaps=0/5453 output_repeats=0
run-2 VALID: new_fps=62.067 capture_misses=231 source_misses=0 torn_frames=0/3724 (0.00%) unreadable_frames=0 max_unreadable_frames=5483
run-2 viewer: startup_ms=737.7 cpu=0.18982281358004097 stalls=0 capture_gaps=0/5483 output_repeats=0
run-3 VALID: new_fps=62.183 capture_misses=237 source_misses=0 torn_frames=0/3732 (0.00%) unreadable_frames=0 max_unreadable_frames=5483
run-3 viewer: startup_ms=719.8 cpu=0.17433616283145817 stalls=0 capture_gaps=0/5483 output_repeats=0
run-4 VALID: new_fps=62.200 capture_misses=229 source_misses=0 torn_frames=0/3732 (0.00%) unreadable_frames=0 max_unreadable_frames=5441
run-4 viewer: startup_ms=721.7 cpu=0.17043608210268768 stalls=0 capture_gaps=0/5441 output_repeats=0
run-5 VALID: new_fps=61.950 capture_misses=234 source_misses=0 torn_frames=0/3718 (0.00%) unreadable_frames=0 max_unreadable_frames=5422
run-5 viewer: startup_ms=689.7 cpu=0.2275594121548716 stalls=0 capture_gaps=0/5422 output_repeats=0
```

## Verdict

The ordinary command exited 1 and correctly refused the comparison:

```sh
stream-cadence verdict /tmp/cadence-vaapi-proof/base-a/results.json /tmp/cadence-vaapi-proof/base-b/results.json
```

Base A ran beside a game in **5/5** runs; base B in **0/5**. What else ran
used **32.8%–34.8%** of all CPUs in A and **0.8%–1.2%** in B. The tool named
both `beside_a_game` and this load difference. These were not comparable
conditions. Queue later had **2/5** runs beside a game. No set was repeated
to obtain a clean result.

For diagnosis only, this local command printed the table with the explicit
conditions warning. It also exited 1. It is **not a fair performance verdict**:

```sh
stream-cadence verdict /tmp/cadence-vaapi-proof/base-a/results.json /tmp/cadence-vaapi-proof/base-b/results.json --allow-different-conditions
```

```text
baseline viewer: copied product package (/nix/store/ycag8ysjam9a02nxjyvfyazqck9vg7vk-moonlight-embedded-korri-2.7.1-korri/bin/moonlight, copied from zao, signed by korri-cache-zao-1, korri-plugins-1), in the fixture, OpenGL on /dev/dri/renderD129
candidate viewer: copied product package (/nix/store/ycag8ysjam9a02nxjyvfyazqck9vg7vk-moonlight-embedded-korri-2.7.1-korri/bin/moonlight, copied from zao, signed by korri-cache-zao-1, korri-plugins-1), in the fixture, OpenGL on /dev/dri/renderD129
The two results ran the same viewer.
baseline picture: requested_platform=x11_vaapi, decode=software
candidate picture: requested_platform=x11_vaapi, decode=software
Allowed by --allow-different-conditions. The runs were not measured under the same conditions: beside_a_game. Some runs were recorded while a game ran on the device, or while its input devices changed, and others were not. A game changes a private pair's numbers; record both results with a game, or both with none. The machine was busier in one result: on the sender device what else ran took 32.8%..34.8% of all CPUs in the baseline and 0.8%..1.2% in the candidate; on the viewer device what else ran took 32.8%..34.8% of all CPUs in the baseline and 0.8%..1.2% in the candidate. Another load on the device changes a pair's numbers; record both results on an equally quiet machine.
what else ran on the machine, share of all CPUs: baseline: sender device 32.8%..34.8%, viewer device 32.8%..34.8%; candidate: sender device 0.8%..1.2%, viewer device 0.8%..1.2%
number | metric | baseline runs | candidate runs | verdict
1 packet arrival to display | packet_to_display_ms | not measured | not measured | not measured
1 sender presentation to display | latency_p50_ms | 14.676..14.907 | 11.800..11.882 | better
1 sender presentation to display | latency_p95_ms | 22.177..22.644 | 18.975..19.042 | better
2 smoothness | dropped_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | repeated_frames | 225.000..227.000 | 231.000..236.000 | worse
2 smoothness | late_frames | 226.000..227.000 | 231.000..236.000 | worse
2 smoothness | torn_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | unreadable_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | output_repeats | 0.000..0.000 | 0.000..0.000 | equal
3 CPU and battery | cpu_seconds_per_second | 0.202..0.205 | 0.156..0.160 | better
3 CPU and battery | battery_mw | not measured | not measured | not measured
4 startup | startup_ms | 689.592..819.139 | 657.303..769.861 | equal
5 input | input_ms | not measured | not measured | not measured
6 stability | stream_stalls | 0.000..0.000 | 0.000..0.000 | equal
6 stability | viewer_exits | 0.000..0.000 | 0.000..0.000 | equal
```

The measured game/load difference confounds the faster delay and lower CPU
in B. The same binary also had more repeated/late frames there. The record
does not prove which mechanism caused each change; these numbers do not
show a viewer-code regression or improvement. The quiet baseline is left
for the owner to record later.

## Isolation and restore

All sixteen runs passed both restore checks. The 2,538 isolation samples
contained 2,523 positive X peer proofs and fifteen startup samples before
an X connection existed. Every run passed its isolation gate. No sample
showed an open `/dev/input` node. The X peer belonged to the private
Xwayland, linked to the fixture compositor by actual socket peer inodes.

Before/after snapshots for all four sets kept the main product service
PIDs/start times and the main product Xwayland `:0` socket endpoints
unchanged. Game service/Xwayland lifetimes were external activity and were
not treated as changes to the main product. No run reported a changed
product service. No private namespace process or run directory remained
after any set.

In the short run's wider pre-build/post-run snapshot, `event17`, `event18`,
`mouse2` and `mouse3` disappeared. Its own run window recorded no input
change. The supervisor identified them as product virtual devices from an
external game/stream test. The three five-run sets had identical input
entries before and after; queue nevertheless recorded game/input activity
inside two runs. All viewer input FD lists stayed empty.

The first short attempt failed closed because the proof reader expected
Xwayland to remain a compositor descendant. wlroots daemonizes it. The
reader now checks the private network namespace and an actual compositor
socket peer instead. An early DBus autolaunch child also briefly outlived
cleanup; explicit nonexistent private session/system bus addresses removed
that autolaunch. The fixed short run and all fifteen set runs left nothing.

Full results, viewer logs, peer proofs, before/after snapshots and leftover
scans are in `/tmp/cadence-vaapi-proof/{short-fixed,base-a,base-b,queue}` and
its sibling `.command.log`, `.before.txt`, `.after.txt`, `.leftovers.txt`
files. The normal and diagnostic verdict outputs are alongside them. No
product service was restarted or reconfigured. No NixOS switch ran. No
plugin source changed, and no other device was contacted.
