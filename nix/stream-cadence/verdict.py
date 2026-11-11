"""Equal, better or worse: one verdict for each number of a viewer measure.

Every number is lower-is-better. Two sets of runs differ only when they do not
overlap: worse when every candidate run is worse than every baseline run,
better when every candidate run is better. Anything else is inside the
run-to-run spread, so it is equal. With five runs each, two sets from the same
viewer separate by chance in about 0.8% of comparisons for each number.
"""

import statistics
from typing import Callable

Value = float | None


def window_median(record: dict, name: str) -> Value:
    values = [
        window["latency_ms"][name]
        for window in record["source_counter"]["windows"]
        if window["latency_ms"][name] is not None
    ]
    return statistics.median(values) if values else None


def window_total(name: str) -> Callable[[dict], Value]:
    return lambda record: sum(
        window[name] for window in record["source_counter"]["windows"]
    )


def measured(name: str) -> Callable[[dict], Value]:
    return lambda record: record["viewer"][name]


# The six numbers of ADR 0062, in order. A number can have several metrics.
METRICS: list[tuple[str, str, Callable[[dict], Value]]] = [
    ("1 packet arrival to display", "packet_to_display_ms", measured("packet_to_display_ms")),
    ("1 sender presentation to display", "latency_p50_ms", lambda r: window_median(r, "p50")),
    ("1 sender presentation to display", "latency_p95_ms", lambda r: window_median(r, "p95")),
    ("2 smoothness", "dropped_frames", window_total("capture_misses")),
    ("2 smoothness", "repeated_frames", window_total("repeated_frames")),
    ("2 smoothness", "late_frames", window_total("late_frames")),
    ("2 smoothness", "torn_frames", window_total("torn_frames")),
    ("2 smoothness", "unreadable_frames", window_total("undecodable_frames")),
    ("3 CPU and battery", "cpu_seconds_per_second", measured("cpu_seconds_per_second")),
    ("3 CPU and battery", "battery_mw", measured("battery_mw")),
    ("4 startup", "startup_ms", measured("startup_ms")),
    ("5 input", "input_ms", measured("input_ms")),
    ("6 stability", "stream_stalls", measured("stream_stalls")),
    ("6 stability", "viewer_exits", measured("viewer_exits")),
]


def verdict(baseline: list[Value], candidate: list[Value]) -> str:
    if any(value is None for value in baseline + candidate):
        return "not measured"
    old = [value for value in baseline if value is not None]
    new = [value for value in candidate if value is not None]
    if min(new) > max(old):
        return "worse"
    if max(new) < min(old):
        return "better"
    return "equal"


def valid_runs(records: list[dict], minimum: int) -> list[dict]:
    runs = [
        record
        for record in records
        if record.get("discovered", {}).get("valid") and "viewer" in record
    ]
    if len(runs) < minimum:
        raise ValueError(
            f"{len(runs)} valid viewer runs; a verdict needs {minimum} in each result"
        )
    return runs


def judge(baseline: list[dict], candidate: list[dict], minimum: int) -> list[dict]:
    old, new = valid_runs(baseline, minimum), valid_runs(candidate, minimum)
    rows = []
    for number, name, extract in METRICS:
        before = [extract(record) for record in old]
        after = [extract(record) for record in new]
        rows.append(
            {
                "number": number,
                "metric": name,
                "baseline": before,
                "candidate": after,
                "verdict": verdict(before, after),
            }
        )
    return rows
