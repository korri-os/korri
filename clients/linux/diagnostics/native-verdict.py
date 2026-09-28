#!/usr/bin/env python3
"""Parse bounded native probe logs off-device. No device or browser operations."""
import argparse
import json
from pathlib import Path
import re

PREFIX = re.compile(r"^\[DEBUG-native-input\] monotonic_us=([0-9]{1,20}) state=(\{.*\})$")
MAX_BYTES = 1_000_000


def integer(value):
    return type(value) is int and 0 < value <= 2_147_483_647


def observations(text):
    if len(text.encode()) > MAX_BYTES:
        raise ValueError("observation log exceeds bound")
    result = []
    for line in text.splitlines():
        if "[DEBUG-native-input]" not in line:
            continue
        match = PREFIX.fullmatch(line)
        if not match or len(line) > 2048:
            raise ValueError("malformed observation")
        state = json.loads(match[2])
        if not isinstance(state, dict):
            raise ValueError("observation is not an object")
        stamp = int(match[1])
        if stamp <= 0 or (result and stamp <= result[-1][0]):
            raise ValueError("non-monotonic observation")
        result.append((stamp, state))
        if len(result) > 480:
            raise ValueError("too many observations")
    return result


def replay_window(text):
    if len(text.encode()) > 8192:
        raise ValueError("replay log exceeds bound")
    def stamp(kind):
        found = re.findall(rf"^REPLAY_{kind}_MONOTONIC_US=([0-9]{{1,20}})$", text, re.M)
        if len(found) != 1:
            raise ValueError("missing or duplicate replay boundary")
        return int(found[0])
    if text.splitlines().count("REPLAY WRITES COMPLETE: four Right pulses; kernel/browser delivery NOT asserted") != 1:
        raise ValueError("replay did not finish safely")
    return stamp("BEGIN"), stamp("END")


def verdict(samples, start, end):
    if not 0 < start < end or end - start > 15_000_000:
        return False, "invalid observation window"
    before = [sample for sample in samples if sample[0] <= start]
    after = [sample for sample in samples if sample[0] >= end]
    if not before or not after:
        return False, "missing bracketing observations"
    chosen = [before[-1], *(s for s in samples if start < s[0] < end), after[0]]
    if len(chosen) < 4 or any(b[0] - a[0] > 1_000_000 for a, b in zip(chosen, chosen[1:])):
        return False, "insufficient observation coverage"
    states = [state for _, state in chosen]
    for state in states:
        if any(state.get(key) is not True for key in ("hasFocus", "visible", "activeIsControl", "countersPresent")):
            return False, "inactive page/control or missing test-only counters"
        for key, expected in (("nativeStarts", 1), ("gamepadStarts", 0), ("gamepadReads", 0)):
            if type(state.get(key)) is not int or state[key] != expected:
                return False, "native-only adapter selection was not observed"
        if not integer(state.get("focusNode")):
            return False, "invalid focus-node identity"
        for key in ("nativeSamples", "nativeInitializations"):
            if type(state.get(key)) is not int or not 0 <= state[key] < 1_000_000:
                return False, "invalid or saturated counter"
        if state["nativeInitializations"] < 1:
            return False, "native attachment was not initialized"
    if any(b["nativeSamples"] < a["nativeSamples"] or b["nativeInitializations"] != a["nativeInitializations"]
           for a, b in zip(states, states[1:])):
        return False, "native attachment restarted or counters decreased"
    # Require a transition observed inside the window, not only after it.
    inside = [s for stamp, s in chosen if start <= stamp <= end]
    if len({s["focusNode"] for s in inside}) < 2:
        return False, "no DOM focus transition observed inside the window"
    if states[-1]["nativeSamples"] <= states[0]["nativeSamples"]:
        return False, "no native input samples observed"
    return True, "native samples and DOM focus movement observed; browser Gamepad read count stayed zero"


def read_bounded(path, limit):
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("log exceeds bound")
    return data.decode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("observations", type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--replay", type=Path)
    mode.add_argument("--physical", nargs=2, type=int, metavar=("START_US", "END_US"))
    args = parser.parse_args()
    try:
        samples = observations(read_bounded(args.observations, MAX_BYTES))
        start, end = replay_window(read_bounded(args.replay, 8192)) if args.replay else args.physical
        passed, reason = verdict(samples, start, end)
    except (ValueError, OSError):
        passed, reason = False, "invalid or unreadable evidence (raw data omitted)"
    print(("PASS: " if passed else "NOT PROVEN: ") + reason)
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
