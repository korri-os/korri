"""Receive-clock windows and presentation-feedback attribution.

Field names come from content-memory-phase-accept-usb1[-source]-result.json.
A discarded submission is not a presentation and can never be a capture miss.
"""

import csv
import math
import re
import statistics
from pathlib import Path
from typing import Callable

from bar import Frame, Torn, Unreadable, frame_ids


class InvalidMeasurement(ValueError):
    """Keep diagnostics without presenting incomplete ID accounting as valid."""

    def __init__(self, message: str, report: dict):
        super().__init__(message)
        self.report = report


def feedback(
    text: str,
) -> tuple[dict[int, dict[str, int]], dict[int, dict[str, int]], set[int]]:
    presented, submitted, discarded = {}, {}, set()
    for line in text.splitlines():
        if line.startswith(("PRESENT ", "SUBMIT ", "DISCARD ")):
            row = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}
            if line.startswith("PRESENT "):
                if row["id"] in presented:
                    raise ValueError("duplicate presentation ID")
                presented[row["id"]] = row
            elif line.startswith("SUBMIT "):
                submitted[row["id"]] = row
            else:
                discarded.add(row["id"])
    if not presented:
        raise ValueError("no compositor presentation feedback")
    if discarded & presented.keys():
        raise ValueError("source both presented and discarded an ID")
    return presented, submitted, discarded


def stamps(path: Path) -> list[dict[str, int]]:
    with path.open() as stream:
        rows = [{k: int(v) for k, v in row.items()} for row in csv.DictReader(stream)]
    if not rows or any(row["index"] != i for i, row in enumerate(rows)):
        raise ValueError("missing or nonconsecutive receive records")
    if any(a["receive_ms"] > b["receive_ms"] for a, b in zip(rows, rows[1:])):
        raise ValueError("receive clock reversed")
    return rows


def bridge(before: dict, after: dict) -> tuple[Callable[[int], int], float]:
    device_a = before["device_ns"]
    device_b = after["device_ns"]
    host_a = (before["host_before_ns"] + before["host_after_ns"]) // 2
    host_b = (after["host_before_ns"] + after["host_after_ns"]) // 2
    if device_b <= device_a or host_b <= host_a:
        raise ValueError("invalid monotonic clock bridge")
    uncertainty = (
        max(
            before["host_after_ns"] - before["host_before_ns"],
            after["host_after_ns"] - after["host_before_ns"],
        )
        / 2e6
    )
    return (
        lambda ns: (
            host_a + (ns - device_a) * (host_b - host_a) // (device_b - device_a)
        ),
        uncertainty,
    )


def latency(values: list[float]) -> dict:
    ordered = sorted(values)
    return {
        "count": len(values),
        "p50": statistics.median(values) if values else None,
        "p95": ordered[math.ceil(0.95 * len(values)) - 1] if values else None,
        "max": max(values) if values else None,
    }


def score(
    ids: list[Frame],
    rows: list[dict[str, int]],
    source: str,
    to_host: Callable[[int], int],
    *,
    warmup: float,
    seconds: float,
    window: float,
    fps: float,
    refresh: float,
    max_unreadable_frames: int = 0,
) -> dict:
    if max_unreadable_frames < 0:
        raise ValueError("max_unreadable_frames must be nonnegative")
    if len(ids) != len(rows):
        raise ValueError(
            f"decode count {len(ids)} differs from receive count {len(rows)}"
        )
    if not rows or any(
        a["receive_ms"] > b["receive_ms"] for a, b in zip(rows, rows[1:])
    ):
        raise ValueError("empty or reversed receive clock")
    count = int(seconds // window)
    duration = count * window
    if count < 1:
        raise ValueError("duration must include at least one complete window")
    origin = rows[0]["receive_ms"] * 1_000_000
    if rows[-1]["receive_ms"] * 1_000_000 < origin + (warmup + duration) * 1e9:
        raise ValueError(
            "recording does not cover warm-up and every complete scored window"
        )
    presented, submitted, discarded = feedback(source)

    def empty(length: float) -> dict:
        return dict(
            capture_misses=0,
            source_misses=0,
            new_frames=0,
            received_frames=0,
            torn_frames=0,
            undecodable_frames=0,
            source_discarded_submissions=0,
            captured_unpresented_ids=0,
            expected_new_frames=min(fps, refresh) * length,
            seconds=length,
        )

    warm = empty(warmup)
    windows = [empty(window) for _ in range(count)]
    latencies: list[list[float]] = [[] for _ in range(count + 1)]

    def bucket(ns: int) -> tuple[dict, list[float]] | None:
        elapsed = (ns - origin) / 1e9
        if 0 <= elapsed < warmup:
            return warm, latencies[0]
        if warmup <= elapsed < warmup + duration:
            index = int((elapsed - warmup) // window)
            return windows[index], latencies[index + 1]
        return None

    # Unreadable frames supply no IDs, including any intact row in that frame.
    # A presented ID seen only there must remain a capture miss, even when the
    # caller accepts an unreadable-frame budget.
    seen = {ident for frame in ids for ident in frame_ids(frame)}
    observed: set[int] = set()
    torn_events = []
    unreadable_events = []
    scored_unreadable = []
    window_numbers = {
        id(values): index for index, values in enumerate([warm, *windows])
    }
    for index, (frame, row) in enumerate(zip(ids, rows)):
        target = bucket(row["receive_ms"] * 1_000_000)
        if isinstance(frame, Unreadable):
            unreadable_events.append(
                {
                    "index": index,
                    "receive_ms": row["receive_ms"],
                    "window": window_numbers[id(target[0])] if target else None,
                    "reason": frame.reason,
                }
            )
        if isinstance(frame, Torn):
            torn_events.append(
                {
                    "index": index,
                    "receive_ms": row["receive_ms"],
                    "window": window_numbers[id(target[0])] if target else None,
                    "upper_id": frame.upper_id,
                    "lower_id": frame.lower_id,
                }
            )
        if target:
            values, delays = target
            values["received_frames"] += 1
            if isinstance(frame, Unreadable):
                values["undecodable_frames"] += 1
                if values is not warm:
                    scored_unreadable.append((index, frame.reason))
            else:
                if isinstance(frame, Torn):
                    values["torn_frames"] += 1
                for ident in frame_ids(frame):
                    if ident in observed:
                        continue
                    values["new_frames"] += 1
                    if ident not in presented:
                        values["captured_unpresented_ids"] += 1
                    else:
                        delays.append(
                            (
                                row["receive_ms"] * 1_000_000
                                - to_host(presented[ident]["present_ns"])
                            )
                            / 1e6
                        )
        observed.update(frame_ids(frame))
    capture_events, source_events = [], []
    for ident, row in sorted(presented.items()):
        target = bucket(to_host(row["present_ns"]))
        if target and ident not in seen:
            target[0]["capture_misses"] += 1
            capture_events.append(
                {"id": ident, "window": window_numbers[id(target[0])], **row}
            )
    ordered = sorted(presented.values(), key=lambda row: row["sequence"])
    for a, b in zip(ordered, ordered[1:]):
        gap = b["sequence"] - a["sequence"]
        if gap <= 0 or b["present_ns"] <= a["present_ns"]:
            raise ValueError("non-monotonic presentation feedback")
        for step in range(1, gap):
            ns = a["present_ns"] + (b["present_ns"] - a["present_ns"]) * step // gap
            target = bucket(to_host(ns))
            if target:
                target[0]["source_misses"] += 1
                source_events.append(
                    {
                        "window": window_numbers[id(target[0])],
                        "missing_vblank_sequence": a["sequence"] + step,
                        "estimated_present_ns": ns,
                        "before_id": a["id"],
                        "after_id": b["id"],
                        "after_submit_ns": b["submit_ns"],
                    }
                )
    for ident in discarded:
        if ident in submitted:
            target = bucket(to_host(submitted[ident]["submit_ns"]))
            if target:
                target[0]["source_discarded_submissions"] += 1
    for values, delays in zip([warm, *windows], latencies):
        values["new_fps"] = (
            values["new_frames"] / values["seconds"] if values["seconds"] else None
        )
        values["latency_ms"] = latency(delays)
    report = dict(
        warmup_seconds=warmup,
        scored_seconds=duration,
        clock="receive_ms with monotonic presentation bridge",
        raw_warmup=warm,
        windows=windows,
        capture_events=capture_events,
        source_events=source_events,
        torn_events=torn_events,
        unreadable_events=unreadable_events,
        torn_frames=sum(values["torn_frames"] for values in windows),
        unreadable_frames=len(scored_unreadable),
        max_unreadable_frames=max_unreadable_frames,
        scored_received_frames=sum(values["received_frames"] for values in windows),
        decode_errors=0,
        decoded_frames=len(ids),
        presented_ids=len(presented),
        capture_gate_pass=all(
            v["capture_misses"] == 0 and v["undecodable_frames"] == 0 for v in windows
        ),
    )
    first_present = to_host(ordered[0]["present_ns"])
    last_present = to_host(ordered[-1]["present_ns"])
    uncovered = [
        index + 1
        for index in range(count)
        if first_present > origin + (warmup + index * window) * 1e9
        or last_present < origin + (warmup + (index + 1) * window) * 1e9
    ]
    if uncovered:
        report["capture_gate_pass"] = False
        raise InvalidMeasurement(
            f"presentation feedback does not cover scored windows {uncovered}: "
            f"feedback spans {(first_present - origin) / 1e9:.6f} to "
            f"{(last_present - origin) / 1e9:.6f} seconds on the receive clock; "
            "cannot attribute the uncovered interval to source or capture",
            report,
        )
    if len(scored_unreadable) > max_unreadable_frames:
        index, reason = scored_unreadable[0]
        raise InvalidMeasurement(
            f"unreadable scored ID at frame {index}: {reason}; "
            f"{len(scored_unreadable)} unreadable frames exceed limit {max_unreadable_frames}",
            report,
        )
    return report
