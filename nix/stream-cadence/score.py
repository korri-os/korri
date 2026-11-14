"""Receive-clock windows and presentation-feedback attribution.

Field names come from content-memory-phase-accept-usb1[-source]-result.json.
A discarded submission is not a presentation and can never be a capture miss.
"""

import bisect
from collections.abc import Collection, Iterable
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


def capture_gaps(
    rows: list[dict[str, int]],
    frames: list[Frame],
    discarded: Collection[int],
    period_ns: float,
    start_ns: float,
    end_ns: float,
    booked: Callable[[int], bool] | None = None,
) -> tuple[int, int]:
    """Gaps in the display record of the scored time: those that lose an ID, and the rest.

    A gap is an interval above 1.5 refresh periods between two records. It
    counts when any part of it lies in the scored time. `rows` and `frames`
    are the whole record.

    score() books a lost ID to a window by the sender's time, and the display
    comes later. So a gap after the scored end also counts when it loses an
    ID that `booked` places inside the scored time: without that, a frame
    that the capture missed there would be one dropped frame of the viewer in
    a valid run. A gap there that cannot be judged counts when any ID that the
    records around it span is booked inside the scored time.

    The source counts its IDs up by one, so an ID is lost when it appears
    nowhere in the record and the sender did not discard it itself. The
    sender's log of presentations is not asked: a log that lacks a line must
    not hide a lost frame.

    Display order places a lost ID only between its neighbours, so each gap is
    judged by the readable records around it: the two before and the two
    after. If an ID between the oldest and the newest of those is lost, the gap
    loses an ID and the record is incomplete there: nothing in the record can
    tell a frame that the capture missed from a frame that the viewer dropped.
    One record of margin on each side is on purpose. A lost ID right beside a
    gap can belong to it: the record cannot say whether the ID after it came
    early, and a torn frame shows two IDs.

    The same four records must be in order. Where they are not, or where no
    readable record stands on one side of the gap, the gap cannot be judged
    and counts as losing an ID. IDs far from a gap decide nothing about it.

    A gap that loses no ID is whole as far as IDs go. At worst the record
    lacks one output frame that repeated a picture.

    Only display times and IDs decide. The capture's request time (`copy_ns`)
    stays in the record as evidence and is not read here: on a real panel two
    commits can be less than one period apart, so a request time proves nothing.
    """
    if len(rows) != len(frames):
        raise ValueError("the display record and its decoded frames differ in length")
    shown = [frame_ids(frame) for frame in frames]
    seen = {ident for ids in shown for ident in ids}
    readable = [index for index, ids in enumerate(shown) if ids]
    incomplete = whole = 0
    for index in range(1, len(rows)):
        before, after = rows[index - 1]["display_ns"], rows[index]["display_ns"]
        if after - before <= 1.5 * period_ns:
            continue
        inside = before < end_ns and after > start_ns
        if not inside and (booked is None or before < end_ns):
            continue
        place = bisect.bisect_left(readable, index)
        older, newer = readable[max(0, place - 2) : place], readable[place : place + 2]
        spans = [(min(shown[at]), max(shown[at])) for at in older + newer]
        judged = bool(older and newer) and all(
            a[0] <= b[0] and a[1] <= b[1] for a, b in zip(spans, spans[1:])
        )
        lost = (
            [
                ident
                for ident in range(spans[0][0] + 1, spans[-1][1])
                if ident not in seen and ident not in discarded
            ]
            if judged
            else []
        )
        if not inside:
            # After the scored end a gap counts for an ID that a scored window
            # holds: a lost one when the gap can be judged. When it cannot, any
            # ID that the records around it span could be the lost one.
            around = [ident for span in spans for ident in span]
            candidates: Iterable[int] = (
                lost if judged else range(min(around), max(around) + 1) if around else []
            )
            if booked is not None and any(booked(ident) for ident in candidates):
                incomplete += 1
        elif not judged or lost:
            incomplete += 1
        else:
            whole += 1
    return incomplete, whole


def first_measured_frame(
    frames: list[Frame],
    presented: dict[int, dict[str, int]],
    launched_ns: int,
    to_host: Callable[[int], int],
) -> int | None:
    """The index of the first output frame that belongs to a measured viewer start.

    It is the first frame with an ID that the sender presented at or after
    that start. A picture that an earlier start left on the display carries an
    older ID and does not count. `launched_ns` and `to_host` share one clock.
    """
    for index, frame in enumerate(frames):
        for ident in frame_ids(frame):
            if ident in presented and to_host(presented[ident]["present_ns"]) >= launched_ns:
                return index
    return None


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


# The viewer's own record of each frame: clients/viewer/picture/frame_times.c
# names each field. Times are the viewer device's CLOCK_MONOTONIC, the clock
# of the display record.
FRAME_TIMES_FIELDS = ["index", "frame_number", "enqueue_ns", "decoded_ns", "display_ns"]


def frame_times(text: str) -> list[dict[str, int]]:
    """The viewer's record: one row for each decode unit, in the order of its recording.

    `decoded_ns` and `display_ns` are absent from a row whose unit the decoder
    returned no frame for, or that no presenter handed to the display.
    """
    reader = csv.DictReader(text.splitlines())
    if reader.fieldnames != FRAME_TIMES_FIELDS:
        raise ValueError(
            f"the viewer's frame times have the fields {reader.fieldnames}, not {FRAME_TIMES_FIELDS}"
        )
    rows = [{name: int(value) for name, value in row.items() if value} for row in reader]
    if not rows or any(row.get("index") != i for i, row in enumerate(rows)):
        raise ValueError("missing or nonconsecutive frame times")
    handed = None
    for row in rows:
        if "frame_number" not in row or "enqueue_ns" not in row:
            raise ValueError(f"frame times without a frame number or arrival at index {row['index']}")
        decoded, display = row.get("decoded_ns"), row.get("display_ns")
        if (decoded is not None and decoded < row["enqueue_ns"]) or (
            display is not None and (decoded is None or display < decoded)
        ):
            raise ValueError(f"frame times out of order at index {row['index']}")
        # Frames reach the display in stream order.
        if display is not None:
            if handed is not None and display <= handed:
                raise ValueError(f"hand-off clock did not advance at index {row['index']}")
            handed = display
    return rows


def packet_to_display(
    units: list[dict[str, int]],
    carried: list[Frame],
    rows: list[dict[str, int]],
    shown: list[Frame],
    start_ns: float,
    end_ns: float,
) -> dict:
    """Number 1 from the viewer's own record, checked against the display record by frame ID.

    `units` is the viewer's record and `carried` the frame that its recording
    decodes to for each unit, in the same order. `rows` and `shown` are the
    display record of the measured start, on the same clock. Each ID that the
    display record shows first inside [start_ns, end_ns) belongs to the first
    unit that carries it and that the viewer handed to the display. Its
    packet-to-display time is that hand-off less the unit's arrival, both from
    the viewer. The check is the capture's display time less the same hand-off:
    a frame cannot be on the display before the viewer handed it over. An ID
    that no handed-off unit carries is unreported.
    """
    if len(units) != len(carried):
        raise ValueError(
            f"the viewer's recording decodes to {len(carried)} frames and its record has {len(units)} units"
        )
    if len(rows) != len(shown):
        raise ValueError("the display record and its decoded frames differ in length")
    handed: dict[int, dict[str, int]] = {}
    for unit, frame in zip(units, carried):
        if "display_ns" in unit:
            for ident in frame_ids(frame):
                handed.setdefault(ident, unit)
    seen: set[int] = set()
    arrival: list[float] = []
    difference: list[float] = []
    unreported: list[int] = []
    for row, frame in zip(rows, shown):
        for ident in frame_ids(frame):
            if ident in seen:
                continue
            seen.add(ident)
            if not start_ns <= row["display_ns"] < end_ns:
                continue
            if ident not in handed:
                unreported.append(ident)
                continue
            unit = handed[ident]
            arrival.append((unit["display_ns"] - unit["enqueue_ns"]) / 1e6)
            difference.append((row["display_ns"] - unit["display_ns"]) / 1e6)
    return {
        "frames": len(arrival),
        "unreported": len(unreported),
        "unreported_ids": unreported[:20],
        "packet_to_display_ms": latency(arrival),
        "handoff_to_display_ms": dict(
            latency(difference), min=min(difference) if difference else None
        ),
        "handoff_after_display": sum(1 for value in difference if value < 0),
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
    source_period_ns: float | None = None,
) -> dict:
    """Score decode units on the receive clock, or output frames on the display clock.

    With `display_period_ns` the rows are a viewer's output frames. Each window
    then also counts frames that repeat an ID and new IDs that arrive late.

    `source_period_ns` is the refresh period of a sender output that counts no
    vblanks, as a headless output does. Its source misses are then derived, not
    counted: each gap above 1.5 periods between two presentations gives
    round(gap / period) - 1 misses.
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
    # An output with no vblank counter reports sequence 0 for every presentation.
    timed = source_period_ns is not None and not any(
        row["sequence"] for row in presented.values()
    )
    ordered = sorted(
        presented.values(), key=lambda row: row["present_ns" if timed else "sequence"]
    )
    for a, b in zip(ordered, ordered[1:]):
        if timed:
            assert source_period_ns is not None
            # More than 1.5 periods between two presentations: the output
            # refreshed in between with no new source frame.
            periods = (b["present_ns"] - a["present_ns"]) / source_period_ns
            gap = max(2, round(periods)) if periods > 1.5 else 1
        else:
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
                        "missing_vblank_sequence": None if timed else a["sequence"] + step,
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


def machine_use(samples: list[dict], start_ns: float, end_ns: float) -> dict | None:
    """How busy the whole machine was between the first and the last look inside a time.

    A look is `device.machine_sample`: the summed CPU counters of /proc/stat,
    the ticks of the agent's own programs, and the load average. `busy` is the
    share of all CPUs that was not idle (waiting for a disk counts as idle),
    and `own` is the share that the agent's own programs used. None when the
    machine was not read twice inside the time.
    """
    inside = [row for row in samples if row and start_ns <= row["ns"] <= end_ns]
    if len(inside) < 2:
        return None
    first, last = inside[0], inside[-1]
    total = sum(last["cpu"]) - sum(first["cpu"])
    seconds = (last["ns"] - first["ns"]) / 1e9
    if total <= 0 or seconds <= 0:
        return None
    # Fields 4 and 5 of the cpu line of proc(5): idle and iowait.
    idle = sum(last["cpu"][3:5]) - sum(first["cpu"][3:5])
    return {
        "busy": (total - idle) / total,
        "own": (last["own_ticks"] - first["own_ticks"]) / total,
        "cpus": total / first["ticks_per_second"] / seconds,
        "seconds": seconds,
        "loadavg_start": first["loadavg"],
        "loadavg_end": last["loadavg"],
    }


