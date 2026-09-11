#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Assert full policy, primitive, scheduler and cleanup evidence in a run.sh log set."""

import json
from pathlib import Path
import sys


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


def samples(path, expected=1):
    observed = [row for row in rows(path) if row.get("kind") == "sample"]
    assert expected > 0, "Expected sample count must be positive"
    assert len(observed) == expected, (
        f"{path.name}: expected {expected} samples, got {len(observed)}"
    )
    assert [row.get("sample") for row in observed] == list(range(expected)), (
        f"{path.name}: incomplete or duplicate sample identities"
    )
    return observed


def check_policy(sample, control):
    assert not any(key.endswith("error") for key in sample), sample
    cases = sample["cases"]
    assert len(cases) == len(control) * 3
    for round_number in range(3):
        observed = {
            case["case"]: case for case in cases if case["round"] == round_number
        }
        assert observed.keys() == control.keys()
        for name, native in control.items():
            case = observed[name]
            assert case["passed"], case
            output = case["output"]
            assert output["ok"] == native["ok"], case
            if output["ok"]:
                assert dict(output["pairs"]) == native["pairs"], case
            else:
                assert output["errorKind"] == native["errorKind"] == "SchemaError", case
                assert output["error"] == native["error"], case
                assert "ReferenceError" not in output["error"], case
    for stage in ["platform_init", "policy_init", "policy_calls"]:
        assert sample[f"timers_after_{stage}"]["scheduled"] == 0
    assert sample["timers_after_cleanup"]["pending"] == 0
    globals_ = json.loads(sample["globals_after_platform"])
    assert globals_["setTimeout"] == globals_["clearTimeout"] == "function"
    assert all(
        globals_[name] == "undefined"
        for name in ["setInterval", "fetch", "process", "require"]
    )


def main(directory, expected_samples):
    expected_samples = int(expected_samples)
    assert expected_samples > 0, "Expected sample count must be positive"
    logs = Path(directory)
    native = rows(logs / "bun-control.jsonl")
    assert len(native) == 14 and all(row["passed"] for row in native)
    control = {row["case"]: row["output"] for row in native}
    assert len(control) == 14, "Native fixture identities must be unique"
    for mode in ["schema", "schema-minify"]:
        for sample in samples(logs / f"{mode}-timers.jsonl", expected_samples):
            check_policy(sample, control)
        for sample in samples(logs / f"{mode}-zero-timer.jsonl"):
            assert "setTimeout is not defined" in sample["init_error"]
            assert not sample.get("cases")
    for sample in samples(logs / "none-baseline.jsonl", expected_samples):
        assert "TextEncoder is not defined" in sample["init_error"]
        assert not sample["timers_enabled"]
    for sample in samples(logs / "primitives.jsonl"):
        checks = sample["primitive_checks"]
        assert len(checks) == 51 and sum(row["passed"] for row in checks) == 48
        assert sample["timers_before_cleanup"]["scheduled"] == 0
    for sample in samples(logs / "timer-checks.jsonl", expected_samples):
        assert all(sample["timer_checks"]["checks"].values())
        assert sample["timer_checks"]["events"] == [
            "zero",
            "microtask",
            "negative",
            "nested",
            "delayed",
        ]
        assert sample["timers_before_cleanup"] == dict(
            scheduled=262, fired=4, cancelled=258, pending=0, highWater=256, limit=256
        )
    for mode in [
        "timer-deadline",
        "timer-nested-deadline",
        "timer-interrupt",
        "timer-throw",
        "timer-cleanup",
    ]:
        for sample in samples(logs / f"{mode}.jsonl", expected_samples):
            assert sample["timers_after_cleanup"]["pending"] == 0, sample
            assert "timer_cleanup_error" not in sample, sample
            if mode == "timer-cleanup":
                assert sample["timer_cleanup_discarded"] == 1
                assert sample["timers_before_cleanup"]["fired"] == 0
            else:
                error = sample["timer_drain_error"]
                expected = (
                    "callback failure"
                    if mode == "timer-throw"
                    else "interrupted"
                    if mode == "timer-interrupt"
                    else "deadline exceeded"
                )
                # Nested work can meet the deadline inside JS or the host loop.
                assert expected in error or (
                    mode == "timer-nested-deadline" and "interrupted" in error
                ), sample
                assert (
                    sample["total_context_init_calls_ms"] < sample["deadline_ms"] + 100
                ), sample
            if mode == "timer-deadline":
                assert sample["timers_before_cleanup"]["fired"] == 0
    print(
        "Verified: 14 policy fixtures x 3 rounds per sample; exact native outputs; 48/51 primitive differential matches; real timers, cancellation, retention and lifetime cleanup."
    )


if __name__ == "__main__":
    main(*sys.argv[1:])
