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


def clock_ns(row: dict[str, int]) -> int:
    """A display record carries nanoseconds; a receive record carries milliseconds."""
    return row["display_ns"] if "display_ns" in row else row["receive_ms"] * 1_000_000


def stamp(row: dict[str, int]) -> dict[str, int]:
    name = "display_ns" if "display_ns" in row else "receive_ms"
    return {name: row[name]}


def display_records(text: str) -> list[dict[str, int]]:
    rows = [{k: int(v) for k, v in row.items()} for row in csv.DictReader(text.splitlines())]
    if any(row["index"] != i for i, row in enumerate(rows)):
        raise ValueError("nonconsecutive display records")
    if any(a["display_ns"] >= b["display_ns"] for a, b in zip(rows, rows[1:])):
        raise ValueError("display clock did not advance")
    return rows


def capture_gaps(rows: list[dict[str, int]], period_ns: float) -> tuple[int, int]:
    """Gaps in the display record: frames the capture missed, and output repeats.

    A gap is an interval above 1.5 refresh periods between two records. The
    compositor copies a frame at its first commit after the capture's request.
    A request sent more than 0.75 period after the previous frame can arrive
    after the next commit, so that gap is a missed frame and the proof needs
    zero of them. After a timely request, a gap means that the compositor
    committed no frame: the display repeated its picture.
    """
    missed = repeats = 0
    for a, b in zip(rows, rows[1:]):
        if b["display_ns"] - a["display_ns"] > 1.5 * period_ns:
            if b["copy_ns"] - a["display_ns"] > 0.75 * period_ns:
                missed += 1
            else:
                repeats += 1
    return missed, repeats


def cpu_rate(
    samples: list[dict], start_ns: int, end_ns: int, ticks_per_second: int
) -> float | None:
    """Viewer CPU seconds for each second, between the first and last scored sample."""
    inside = [
        row
        for row in samples
        if start_ns <= row["device_ns"] <= end_ns and row["viewer_ticks"] is not None
    ]
    if len(inside) < 2 or inside[-1]["device_ns"] <= inside[0]["device_ns"]:
        return None
    used = (inside[-1]["viewer_ticks"] - inside[0]["viewer_ticks"]) / ticks_per_second
    return used / ((inside[-1]["device_ns"] - inside[0]["device_ns"]) / 1e9)


def battery_mw(samples: list[dict], start_ns: int, end_ns: int) -> float | None:
    """Average battery power, only when a battery discharges in every scored sample."""
    powers = []
    for row in samples:
        if not start_ns <= row["device_ns"] <= end_ns:
            continue
        cells = row["battery"]
        if not cells or any(cell["status"] != "Discharging" for cell in cells):
            return None
        total = 0.0
        for cell in cells:
            if cell["power_now"] is not None:
                total += abs(cell["power_now"]) / 1e3
            elif cell["current_now"] is not None and cell["voltage_now"] is not None:
                total += abs(cell["current_now"]) * cell["voltage_now"] / 1e9
            else:
                return None
        powers.append(total)
    return statistics.fmean(powers) if powers else None


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
    display_period_ns: float | None = None,
) -> dict:
    """Score decode units on the receive clock, or output frames on the display clock.

    With `display_period_ns` the rows are a viewer's output frames. Each window
    then also counts frames that repeat an ID and new IDs that arrive late.
    """
    if max_unreadable_frames < 0:
        raise ValueError("max_unreadable_frames must be nonnegative")
    if len(ids) != len(rows):
        raise ValueError(
            f"decode count {len(ids)} differs from receive count {len(rows)}"
        )
    if not rows or any(clock_ns(a) > clock_ns(b) for a, b in zip(rows, rows[1:])):
        raise ValueError("empty or reversed receive clock")
    count = int(seconds // window)
    duration = count * window
    if count < 1:
        raise ValueError("duration must include at least one complete window")
    origin = clock_ns(rows[0])
    if clock_ns(rows[-1]) < origin + (warmup + duration) * 1e9:
        raise ValueError(
            "recording does not cover warm-up and every complete scored window"
        )
    presented, submitted, discarded = feedback(source)

    def empty(length: float) -> dict:
        display = (
            dict(repeated_frames=0, late_frames=0) if display_period_ns is not None else {}
        )
        return dict(
            **display,
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
    last_new_ns: int | None = None
    stalls = 0
    for index, (frame, row) in enumerate(zip(ids, rows)):
        target = bucket(clock_ns(row))
        fresh = [ident for ident in frame_ids(frame) if ident not in observed]
        if display_period_ns is not None and not isinstance(frame, Unreadable):
            if fresh:
                gap = clock_ns(row) - last_new_ns if last_new_ns is not None else 0
                if target and gap > 1.5 * display_period_ns:
                    target[0]["late_frames"] += 1
                # A stall is a second without a new ID while the stream is scored.
                if target and target[0] is not warm and gap >= 1e9:
                    stalls += 1
                last_new_ns = clock_ns(row)
            elif target:
                target[0]["repeated_frames"] += 1
        if isinstance(frame, Unreadable):
            unreadable_events.append(
                {
                    "index": index,
                    **stamp(row),
                    "window": window_numbers[id(target[0])] if target else None,
                    "reason": frame.reason,
                }
            )
        if isinstance(frame, Torn):
            torn_events.append(
                {
                    "index": index,
                    **stamp(row),
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
                            (clock_ns(row) - to_host(presented[ident]["present_ns"])) / 1e6
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
        clock=(
            "display_ns with monotonic presentation bridge"
            if display_period_ns is not None
            else "receive_ms with monotonic presentation bridge"
        ),
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
    if display_period_ns is not None:
        report["stream_stalls"] = stalls
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
