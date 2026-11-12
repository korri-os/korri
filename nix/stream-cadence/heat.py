"""The heat guard's rules: what a device's own thermal zones allow.

Both agents use them. Each limit comes from the trip points that the device
itself reports at run time; the rules here add only margins. No temperature
of a device is in this file.
"""

from pathlib import Path

THERMAL = Path("/sys/class/thermal")
END_MARGIN = 10_000  # millidegrees: a run ends at least this far below a critical trip


def thermal_zones() -> list[dict]:
    """Each thermal zone with its temperature and trip points, in millidegrees Celsius.

    No zone is dropped. What cannot be read is None (`temp`, or `trips` as a
    whole), and `unreadable` says why: the guard must know that it is blind.
    """
    zones = []
    for zone in sorted(THERMAL.glob("thermal_zone*")):
        row: dict = {
            "zone": zone.name,
            "type": zone.name,
            "temp": None,
            "trips": None,
            "unreadable": None,
        }
        try:
            row["type"] = (zone / "type").read_text().strip()
            row["trips"] = [
                {
                    "type": kind.read_text().strip(),
                    "temp": int(kind.with_name(kind.name[: -len("type")] + "temp").read_text()),
                }
                for kind in sorted(zone.glob("trip_point_*_type"))
            ]
        except (OSError, ValueError) as error:
            row["trips"] = None
            row["unreadable"] = f"its trip points: {error}"
        try:
            row["temp"] = int((zone / "temp").read_text())
        except (OSError, ValueError) as error:
            row["unreadable"] = row["unreadable"] or f"its temperature: {error}"
        zones.append(row)
    return zones


def blind(zones: list[dict], limits: dict[str, dict | None]) -> str | None:
    """Why the guard cannot judge a zone that it must judge, or None.

    `limits` holds every zone that the agent found at its start. A zone that
    has a limit must still be there with a temperature. A zone whose trip
    points cannot be read could hold a critical one, so it blinds the guard
    too. A zone with readable trip points and no critical one decides nothing.
    """
    now = {zone["zone"]: zone for zone in zones}
    for name, limit in limits.items():
        if name not in now:
            if limit:
                return f"The heat guard cannot read {name}: the zone is gone."
            continue
    for zone in zones:
        judged = limits.get(zone["zone"])
        if zone["trips"] is None or (judged and zone["temp"] is None):
            return (
                f"The heat guard cannot read {zone['type']} ({zone['zone']}): "
                f"{zone['unreadable']}."
            )
    return None


def thermal_limits(zone: dict, end_margin: int = END_MARGIN) -> dict | None:
    """When a run may start and when it must end, from one zone's own trip points.

    A zone with no critical trip point gives no limit. A run ends END_MARGIN
    below the critical trip, or half-way between the highest passive trip and
    the critical trip when that is lower. A run does not start at or above the
    lowest passive trip: the device already slows itself there. With no
    passive trip a run does not start within two margins of the critical trip.
    Trip points at or below zero, and passive trips not below the critical
    one, are placeholders and count as absent.
    """
    if zone["trips"] is None:
        return None
    critical = min(
        (trip["temp"] for trip in zone["trips"] if trip["type"] == "critical" and trip["temp"] > 0),
        default=None,
    )
    if critical is None:
        return None
    passive = [
        trip["temp"]
        for trip in zone["trips"]
        if trip["type"] == "passive" and 0 < trip["temp"] < critical
    ]

    def end_at(margin: int) -> int:
        # A test may ask for a wider margin, never for a narrower one.
        margin = max(margin, END_MARGIN)
        return critical - max(margin, (critical - max(passive)) // 2 if passive else 0)

    start = min(passive) if passive else critical - 2 * END_MARGIN
    return {
        "critical": critical,
        "start_below": min(start, end_at(END_MARGIN)),
        "end_at": end_at(end_margin),
    }


def too_hot(zones: list[dict], limits: dict[str, dict | None], rule: str) -> str | None:
    """Why the device is too hot under one rule (start_below or end_at), or None."""
    for zone in zones:
        limit = limits.get(zone["zone"])
        if limit and zone["temp"] is not None and zone["temp"] >= limit[rule]:
            return (
                f"{zone['type']} reads {zone['temp'] / 1000:.1f} °C. "
                + (
                    "A run does not start at or above"
                    if rule == "start_below"
                    else "A run ends at"
                )
                + f" {limit[rule] / 1000:.1f} °C; the device's critical trip point is "
                f"{limit['critical'] / 1000:.1f} °C."
            )
    return None


def no_start(zones: list[dict], limits: dict[str, dict | None], hardware_path: bool) -> str | None:
    """Why a run must not start on this device, or None.

    The guard is blind, the device has a hardware picture path and no zone
    gives a limit, or the device is hot already.
    """
    unseen = blind(zones, limits)
    if unseen:
        return unseen + " A run does not start."
    if hardware_path and not any(limits.values()):
        return (
            "This device has a hardware picture path and no thermal zone with a critical "
            "trip point. The heat guard has no limit for it, so a run does not start."
        )
    hot = too_hot(zones, limits, "start_below")
    if hot:
        return "The device is hot. " + hot + " Let it cool, then run again."
    return None


def must_end(zones: list[dict], limits: dict[str, dict | None]) -> str | None:
    """Why what runs must stop for heat: the guard is blind, or the device is near its critical trip."""
    unseen = blind(zones, limits)
    if unseen:
        return unseen + " The run ends."
    return too_hot(zones, limits, "end_at")
