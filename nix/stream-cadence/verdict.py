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


def viewer_drops(record: dict) -> Value:
    """Frames that the sender presented and the viewer did not show.

    A result can say that this count is not the viewer's: in the queue measure
    the sender shows more frames than the viewer can. The count then stays in
    the windows as evidence and has no verdict.
    """
    if "dropped_frames" in (record["viewer"].get("not_measured") or {}):
        return None
    return window_total("capture_misses")(record)


def measured(name: str) -> Callable[[dict], Value]:
    # A result from before a metric existed has no value for it.
    return lambda record: record["viewer"].get(name)


# The six numbers of ADR 0062, in order. A number can have several metrics.
METRICS: list[tuple[str, str, Callable[[dict], Value]]] = [
    ("1 packet arrival to display", "packet_to_display_ms", measured("packet_to_display_ms")),
    ("1 sender presentation to display", "latency_p50_ms", lambda r: window_median(r, "p50")),
    ("1 sender presentation to display", "latency_p95_ms", lambda r: window_median(r, "p95")),
    ("2 smoothness", "dropped_frames", viewer_drops),
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
# The two measures of a private pair, in `discovered.measure`. The base measure
# is the default: its sender is the slower side. The queue measure's sender is
# the faster side (cli.py: pair_rate). They answer different questions.
BASE = "base"
QUEUE = "queue"
# Which viewer ran and where, in `discovered.viewer_ran`.
DISPLAY, FIXTURE = "display", "fixture"
PLUGIN_VIEWER, STAND_IN = "plugin", "stand-in"
# A product viewer package that the tool copied to a device with no plugin.
COPIED_PRODUCT = "copied-product"
# The lists of game units in `discovered.live_game`: each device, start and end.
LIVE_GAME_MOMENTS = ("sender_at_start", "viewer_at_start", "sender_at_end", "viewer_at_end")
# The entries under /dev/input that changed during the run, in the same place.
LIVE_GAME_INPUTS = ("input_devices_appeared", "input_devices_disappeared")
# The devices in `discovered.machine`: how busy each was, and what else ran there.
MACHINE_SIDES = ("sender", "viewer")


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
    # The units are read at the start and at the end. A game that starts or ends
    # between the two adds or removes entries under /dev/input, so such an
    # entry counts as a game too; a controller that a person plugs in does as well.
    live = facts.get("live_game") or {}
    values["beside_a_game"] = any(
        live.get(evidence) for evidence in (*LIVE_GAME_MOMENTS, *LIVE_GAME_INPUTS)
    )
    # A result from before the tool named its measure is a base result: the
    # queue measure did not exist.
    values["measure"] = facts.get("measure") or BASE
    # A headless output of the fixture and the device's own panel are two
    # displays. Which viewer ran is no condition (see `viewers`).
    values["viewer_place"] = viewer_of(record)["place"]
    # What drew the fixture's output. A result from before the fact ran with
    # pixman: the fixture had no other renderer. None on the device's display.
    values["viewer_renderer"] = (
        (facts.get("viewer_fixture") or {}).get("renderer") or "pixman"
        if values["viewer_place"] == FIXTURE
        else None
    )
    return values


def viewer_of(record: dict) -> dict:
    """Which viewer a run measured and where it showed the stream.

    `discovered.viewer_ran` says it. A result from before that fact is read
    from what it has: then a device with the plugin ran the plugin on its own
    display, and any other device ran the stand-in in the fixture.
    """
    facts = record["discovered"]
    ran = facts.get("viewer_ran")
    if ran:
        return ran
    plugin = (facts.get("viewer") or {}).get("plugin")
    if plugin:
        return {
            "kind": PLUGIN_VIEWER,
            "place": DISPLAY,
            "moonlight": plugin.get("moonlight"),
            "release_version": plugin.get("release_version"),
            "package": plugin.get("package"),
        }
    command = facts.get("viewer_command") or [None]
    return {
        "kind": STAND_IN,
        "place": FIXTURE,
        "moonlight": command[0],
        "release_version": None,
        "package": None,
    }


def one_viewer(records: list[dict], name: str) -> dict:
    """The one viewer of a result; a result with more than one viewer is no result of a viewer."""
    found = {json.dumps(viewer_of(record), sort_keys=True) for record in records}
    if len(found) != 1:
        raise ValueError(
            f"The {name} result has runs of more than one viewer: "
            + "; ".join(sorted(described(json.loads(row)) for row in found))
            + ". A verdict needs one viewer in each result."
        )
    return json.loads(found.pop())


def described(ran: dict) -> str:
    name = {
        PLUGIN_VIEWER: f"plugin {ran.get('release_version')}",
        COPIED_PRODUCT: "copied product package",
    }.get(ran["kind"], "stand-in")
    origin = ""
    if ran["kind"] == COPIED_PRODUCT:
        signed = ", ".join(ran.get("signed_by") or []) or "no key"
        origin = f", copied from {ran.get('copied_from')}, signed by {signed}"
    place = "in the fixture" if ran["place"] == FIXTURE else "on the device's own display"
    # The render nodes that the viewer held open: where its own OpenGL ran. A
    # result from before that read has no word about it.
    nodes = ran.get("render_nodes")
    opengl = (
        ""
        if nodes is None
        else f", OpenGL on {', '.join(nodes)}"
        if nodes
        else ", no render node open: its OpenGL is Mesa's software rasterizer, or it has none"
    )
    return f"{name} ({ran.get('moonlight')}{origin}), {place}{opengl}"


def viewers(baseline: list[dict], candidate: list[dict]) -> list[str]:
    """Which viewer each result ran, for the head of a verdict.

    The viewer is what a comparison changes on purpose, so it is no condition:
    two different viewers are compared. The verdict says which they are, so
    that nobody reads a stand-in's numbers as the product viewer's.
    """
    old, new = one_viewer(baseline, "baseline"), one_viewer(candidate, "candidate")
    if old == new:
        last = "The two results ran the same viewer."
    elif old.get("moonlight") == new.get("moonlight") and old.get("render_nodes") != new.get("render_nodes"):
        last = (
            "The two results ran the same viewer binary, but its OpenGL differs: one drew on "
            "the graphics card and one did not. Every verdict below compares these two."
        )
    elif old.get("moonlight") == new.get("moonlight"):
        last = (
            "The two results ran the same viewer binary; how it came to the device, or where "
            "it showed the stream, differs."
        )
    else:
        last = "The two results ran different viewers; every verdict below compares these two."
    return ["baseline viewer: " + described(old), "candidate viewer: " + described(new), last]


def product_changes(baseline: list[dict], candidate: list[dict]) -> list[str]:
    """What happened to the product's own processes during the runs, for lines of their own.

    A run in which a product unit restarted, appeared or went stays valid:
    the pair does not use the product. But the device was not the same
    during that run, and a reader of the verdict must know it.
    """
    lines = []
    for name, records in (("baseline", baseline), ("candidate", candidate)):
        kept = [record for record in records if "product_units" in record["discovered"]]
        if not kept:
            lines.append(f"{name}: the product's units were not recorded.")
            continue
        for record in kept:
            sides = record["discovered"]["product_units"] or {}
            changed = sorted(
                {unit for side in sides.values() for unit in ((side or {}).get("changed") or [])}
            )
            if changed:
                lines.append(
                    f"{name}: the product changed during {record.get('label')}: "
                    + ", ".join(changed)
                    + ". The run is valid and is in the verdict."
                )
    return lines


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


def conditions_that_differ(old: list[dict], new: list[dict]) -> str | None:
    """Which conditions differ between two results of valid runs, in words; None when none does.

    The refusal of a verdict, and the line that a verdict prints when the
    override lets it go on: a difference must be named either way.
    """
    changed = differences(old, new)
    busier = machine_difference(old, new)
    if not changed:
        return busier
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
    # These have their own words. The measure also sets the rate and the mode.
    named = {"beside_a_game", "viewer_place", "viewer_renderer"}
    if "measure" in changed:
        named |= {"measure", "requested_fps", "display"}
    differ = [name for name in changed if name not in absent and name not in named]
    advice = []
    if "viewer_renderer" in changed:
        advice.append(
            "Some runs drew the viewer fixture's output with pixman, where a viewer's "
            "OpenGL is Mesa's software rasterizer, and others with gles2 on the graphics "
            "card. A result from before the tool recorded the renderer ran with pixman. "
            "Record both results with the same renderer."
        )
    if "viewer_place" in changed:
        advice.append(
            "Some runs showed the stream in the fixture and others on the device's own "
            "display. These are two displays; record both results in the same place."
        )
    if "measure" in changed:
        advice.append(
            "Some runs are the base measure of a private pair and others are its queue "
            "measure (--queue-measure). The base measure shows the delay with no queue and "
            "the viewer's own drops; the queue measure shows the delay of a full queue. "
            "Compare base with base, and queue with queue."
        )
    if "beside_a_game" in changed:
        advice.append(
            "Some runs were recorded while a game ran on the device, or while its input "
            "devices changed, and others were not. "
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
    return (
        "The runs were not measured under the same conditions: "
        + ", ".join(changed)
        + ". "
        + " ".join(advice)
        + (" " + busier if busier else "")
    )


def judge(
    baseline: list[dict],
    candidate: list[dict],
    minimum: int,
    allow_different_conditions: bool = False,
) -> list[dict]:
    old, new = valid_runs(baseline, minimum), valid_runs(candidate, minimum)
    one_viewer(old, "baseline")
    one_viewer(new, "candidate")
    words = conditions_that_differ(old, new)
    if words and not allow_different_conditions:
        raise ValueError(words)
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


def machine_others(records: list[dict], side: str) -> list[float] | None:
    """What else ran on one device in each run of a result; None when any run lacks the record."""
    values = [((record["discovered"].get("machine") or {}).get(side) or {}).get("other") for record in records]
    known = [float(value) for value in values if value is not None]
    return known if len(known) == len(values) else None


def share(values: list[float]) -> str:
    return f"{min(values) * 100:.1f}%..{max(values) * 100:.1f}%"


def machine_line(baseline: list[dict], candidate: list[dict]) -> str:
    """One line for a verdict: what else ran on each device, as a share of all CPUs, in both results."""
    parts = []
    for name, records in (("baseline", baseline), ("candidate", candidate)):
        sides = [
            f"{side} device {share(values)}"
            for side in MACHINE_SIDES
            if (values := machine_others(records, side)) is not None
        ]
        parts.append(f"{name}: " + (", ".join(sides) if sides else "not recorded"))
    return "what else ran on the machine, share of all CPUs: " + "; ".join(parts)


# The smallest gap, as a share of all CPUs, between the two ranges of what
# else ran that makes two results differ. A tool rule chosen without data: the
# one loaded case (about 2.5 of 12 CPUs on aka, a gap near 20 points) moved the
# delay by 8 ms, and a quiet aka reads about 3%. Revisit it when a result shows
# a smaller gap with a clear effect on the numbers.
MACHINE_SMALLEST_GAP = 0.05


def machine_difference(old: list[dict], new: list[dict]) -> str | None:
    """Why the two results were not measured on an equally busy machine, or None.

    The rule of a metric, with a smallest gap: two ranges differ when they do
    not overlap and lie at least MACHINE_SMALLEST_GAP apart. Another
    load on the device moves every number of a pair, so a verdict between such
    results would judge that load. A result that has no record while the
    other has one was recorded before the tool kept it.
    """
    words = []
    for side in MACHINE_SIDES:
        before, after = machine_others(old, side), machine_others(new, side)
        if before is None and after is None:
            continue
        if before is None or after is None:
            return (
                "One result was recorded before the tool kept how busy the machine was "
                "(machine); record that result again."
            )
        gap = max(min(after) - max(before), min(before) - max(after))
        if gap >= MACHINE_SMALLEST_GAP - 1e-9:
            words.append(
                f"on the {side} device what else ran took {share(before)} of all CPUs in the "
                f"baseline and {share(after)} in the candidate"
            )
    if not words:
        return None
    return (
        "The machine was busier in one result: "
        + "; ".join(words)
        + ". Another load on the device changes a pair's numbers; record both results on an "
        "equally quiet machine."
    )


