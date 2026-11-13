"""Shared heat rules for thermal zones and hwmon CPU package sensors.

Reported limits take margins. AMD package sensors without reported limits
use the fixed 85/100 °C start/stop rule documented in the README.
"""

from pathlib import Path

THERMAL = Path("/sys/class/thermal")
END_MARGIN = 10_000  # millidegrees: a run ends at least this far below a critical trip
AMD_START_BELOW = 85_000
AMD_END_AT = 100_000


def cpu_sensors() -> list[dict]:
    """Read only CPU package labels of known hwmon drivers, in the zone fact shape.

    hwmon is a sibling of the thermal class, including in sysfs fixture trees.
    Prefer AMD's die temperature to its control temperature when both exist.
    A selected sensor with unreadable input or limits remains visible and blinds
    the guard, just like an unreadable thermal zone.
    """
    sensors = []
    for device in sorted((THERMAL.parent / "hwmon").glob("hwmon*")):
        try:
            driver = (device / "name").read_text().strip()
        except OSError:
            continue
        if driver not in ("k10temp", "coretemp", "zenpower"):
            continue
        labels = {}
        for path in sorted(device.glob("temp*_label")):
            try:
                labels[path] = path.read_text().strip()
            except OSError as error:
                # This could be the package sensor; do not silently run unguarded.
                sensors.append({
                    "zone": f"{device.name}/{path.name.removesuffix('_label')}",
                    "type": driver,
                    "driver": driver,
                    "temp": None,
                    "trips": None,
                    "unreadable": f"its sensor label: {error}",
                })
        wanted = "Package id 0" if driver == "coretemp" else (
            "Tdie" if "Tdie" in labels.values() else "Tctl"
        )
        for path, label in labels.items():
            if label != wanted:
                continue
            stem = path.name.removesuffix("_label")
            row: dict = {
                "zone": f"{device.name}/{stem}",
                "type": f"{driver} {label}",
                "driver": driver,
                "temp": None,
                "trips": [],
                "unreadable": None,
            }
            try:
                for kind in ("crit", "max"):
                    try:
                        limit = int((device / f"{stem}_{kind}").read_text())
                    except FileNotFoundError:
                        continue
                    if limit > 0:
                        row["trips"] = [{"type": "critical", "temp": limit}]
                        break
            except (OSError, ValueError) as error:
                row["trips"] = None
                row["unreadable"] = f"its limits: {error}"
            try:
                row["temp"] = int((device / f"{stem}_input").read_text())
            except (OSError, ValueError) as error:
                row["unreadable"] = row["unreadable"] or f"its temperature: {error}"
            sensors.append(row)
    return sensors


def thermal_zones() -> list[dict]:
    """Thermal zones and CPU sensors, with temperatures and limits in millidegrees Celsius.

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
    return zones + cpu_sensors()


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
    """Start/stop limits from reported trips, or the fixed rule for unbounded AMD sensors.

    A thermal zone with no critical trip point gives no limit. A run ends END_MARGIN
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
        if zone.get("driver") in ("k10temp", "zenpower"):
            return {
                "critical": None,
                "start_below": AMD_START_BELOW,
                "end_at": AMD_END_AT - (max(end_margin, END_MARGIN) - END_MARGIN),
            }
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
                + f" {limit[rule] / 1000:.1f} °C; "
                + (
                    f"the device's critical trip point is {limit['critical'] / 1000:.1f} °C."
                    if limit["critical"] is not None
                    else "the AMD package sensor has no reported limit, so the fixed margin rule applies."
                )
            )
    return None


def no_start(
    zones: list[dict], limits: dict[str, dict | None], hardware_path: bool,
    *, check_temperature: bool = True,
) -> str | None:
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
            "trip point or CPU package sensor with a limit. The heat guard has no limit "
            "for it, so a run does not start."
        )
    hot = too_hot(zones, limits, "start_below") if check_temperature else None
    if hot:
        return "The device is hot. " + hot + " Let it cool, then run again."
    return None


# A wait between runs ends this far below the start limit, so that the start
# check right after it does not refuse on a reading at the limit's edge.
COOLDOWN_MARGIN = 3_000


def start_status(limits: dict[str, dict | None], hardware_path: bool) -> dict:
    """Fresh start temperatures; unsafe/unreadable guards fail instead of waiting."""
    zones = thermal_zones()
    refused = no_start(zones, limits, hardware_path, check_temperature=False)
    if refused:
        raise RuntimeError(refused)
    cooler = {
        name: dict(limit, start_below=limit["start_below"] - COOLDOWN_MARGIN) if limit else limit
        for name, limit in limits.items()
    }
    return {
        "thermal": [dict(zone, limits=limits.get(zone["zone"])) for zone in zones],
        "hot": too_hot(zones, cooler, "start_below"),
    }


def must_end(zones: list[dict], limits: dict[str, dict | None]) -> str | None:
    """Why what runs must stop for heat: the guard is blind, or the device is near its critical trip."""
    unseen = blind(zones, limits)
    if unseen:
        return unseen + " The run ends."
    return too_hot(zones, limits, "end_at")
