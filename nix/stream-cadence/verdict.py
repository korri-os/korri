"""Equal, better or worse: one verdict for each number of a viewer measure.

Every number is lower-is-better. Two sets of runs differ only when they do not
overlap: worse when every candidate run is worse than every baseline run,
better when every candidate run is better. Anything else is inside the
run-to-run spread, so it is equal. With five runs each, two sets from the same
viewer separate by chance in about 0.8% of comparisons for each number.
"""

import json
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
    # A result from before a metric existed has no value for it.
    return lambda record: record["viewer"].get(name)


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
    ("2 smoothness", "output_repeats", measured("output_repeats")),
    ("3 CPU and battery", "cpu_seconds_per_second", measured("cpu_seconds_per_second")),
    ("3 CPU and battery", "battery_mw", measured("battery_mw")),
    ("4 startup", "startup_ms", measured("startup_ms")),
    ("5 input", "input_ms", measured("input_ms")),
    ("6 stability", "stream_stalls", measured("stream_stalls")),
    ("6 stability", "viewer_exits", measured("viewer_exits")),
]


# What the sender and the request must share for two results to compare viewers.
CONDITIONS = (
    "display",
    "requested_resolution",
    "requested_fps",
    "requested_codec",
    "requested_bitrate",
    "viewer_refresh_hz",
    "sunshine_version",
    "encoder",
    "capture_backend",
    # A result that was recorded with a raised limit for gaps that lose an ID
    # was valid by a looser rule.
    "max_capture_gaps",
)
# Conditions that every result of today's tool has and an older result lacks.
# Only for these can one result be older than the condition. A value that is
# absent for any other condition is a value: a product sender has no render node.
ADDED_LATER = ("viewer_model",)
# The lists of game units in `discovered.live_game`: each device, start and end.
LIVE_GAME_MOMENTS = ("sender_at_start", "viewer_at_start", "sender_at_end", "viewer_at_end")


def conditions(record: dict) -> dict:
    facts = record["discovered"]
    values = {name: facts.get(name) for name in CONDITIONS}
    # Only the mode decides the sender's work; power state and name do not.
    if isinstance(values["display"], dict):
        values["display"] = values["display"].get("mode")
    # Which machines did the work: two hosts or two devices are two conditions.
    # A result from before a fact existed has no value for it.
    values["sender_model"] = facts.get("model")
    values["sender_render_node"] = (facts.get("sender_fixture") or {}).get("render_node")
    values["viewer_model"] = (facts.get("viewer") or {}).get("model")
    # A game on the device changes the numbers of a private pair. A result from
    # before this record was kept ran with no game: the tool refused beside one.
    live = facts.get("live_game") or {}
    values["beside_a_game"] = any(live.get(moment) for moment in LIVE_GAME_MOMENTS)
    return values


def differences(baseline: list[dict], candidate: list[dict]) -> list[str]:
    """Conditions that differ between any two runs; a verdict then judges the sender too."""
    reference = conditions(baseline[0])
    return sorted(
        {
            name
            for record in baseline + candidate
            for name, value in conditions(record).items()
            if value != reference[name]
        }
    )


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


def judge(
    baseline: list[dict],
    candidate: list[dict],
    minimum: int,
    allow_different_conditions: bool = False,
) -> list[dict]:
    old, new = valid_runs(baseline, minimum), valid_runs(candidate, minimum)
    changed = differences(old, new)
    if changed and not allow_different_conditions:
        # A condition that some runs have and others lack was added to the tool
        # between the two recordings. That is another cause than another sender.
        every = [conditions(record) for record in old + new]
        absent = [
            name
            for name in changed
            if name in ADDED_LATER
            and any(row[name] is None for row in every)
            and len({json.dumps(row[name], sort_keys=True) for row in every if row[name] is not None}) == 1
        ]
        differ = [name for name in changed if name not in absent and name != "beside_a_game"]
        advice = []
        if "beside_a_game" in changed:
            advice.append(
                "Some runs were recorded while a game ran on the device and others were not. "
                "A game changes a private pair's numbers; record both results with a game, "
                "or both with none."
            )
        if differ:
            advice.append(
                "Record the baseline and the candidate with the same sender, request and devices."
            )
        if absent:
            advice.append(
                "One result was recorded before the tool kept "
                + ", ".join(absent)
                + "; record that result again."
            )
        raise ValueError(
            "The runs were not measured under the same conditions: "
            + ", ".join(changed)
            + ". "
            + " ".join(advice)
        )
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
