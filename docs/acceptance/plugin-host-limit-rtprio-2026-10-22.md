# Bounded native plugin RT allowance — 2026-10-22

Verified: native units can explicitly request `LimitRTPRIO=1` through `20`.
The publisher's measured consumer needs 15 for Sunshine's capture thread.
This does not admit service-wide scheduling policy or `CAP_SYS_NICE`.

The native source remains the configuration owner. The existing privileged
request list, warning and complete unit configuration expose the allowance
before approval. The approval digest changes when the allowance is added.
The host derives `RestrictRealtime=no` only for a requesting unit; otherwise
its existing hardening and approval remain byte-for-byte unchanged.

| Check | Result |
| --- | --- |
| New parser regression before implementation | Failed on unsupported `LimitRTPRIO: 1`, as expected. |
| New hardening regression before implementation | Failed because the RT filter remained enabled, as expected. |
| Plugin-host tests, serial final run | 220 passed; 16 ignored. |
| `cargo check --locked` | Passed. |
| `cargo clippy --locked --lib -- -D warnings` | Passed. |
| `cargo fmt` | Passed. |
| Real-store graph with built dependency fixtures | Passed (one ignored-by-default real-store test). |
| `korri-plugin-builder` | Passed with immutable real-store fixtures. |
| `korri-runtime-plugin-host` VM, final source | Passed: native unit admission, lifecycle, approval, signature and recovery gates. |
| All-target Clippy | Blocked by existing `items_after_test_module` in `src/main.rs`; not changed here. |

The first unrestricted-parallel final test run failed the existing
`host_recovers_a_sigkilled_removal_from_the_durable_original_request` test
while builds were running. The serial rerun passed all 220 tests. The cause
of that first failure is not proven. The first real-store invocation lacked
its required fixture environment; the documented invocation passed.

The native grammar rejects empty reset, zero, negative/over-20 values,
ranges, `infinity`, soft:hard pairs and multiple tokens. Tests also reject
`CPUScheduling*`, `CAP_SYS_NICE`, `Nice` and `IOScheduling*`. Duplicate scalar
assignments use systemd's last-wins behavior in the approval request list.

Risk: the allowance lets any thread in the approved service request bounded
FIFO/RR. It does not guarantee only Sunshine's capture thread uses it, and
FIFO can delay lower-priority input/compositor work. Device acceptance must
measure responsiveness. The publisher change is separate; its locked Core
input remains unchanged until this commit is reviewed and reachable. No
publication, device host update, default pin update or reboot was performed.
