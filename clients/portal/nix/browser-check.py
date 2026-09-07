#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Check the actual kiosk through a temporary, loopback-only CDP connection."""

import argparse
import json
import pathlib
import subprocess


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cdp", required=True)
    parser.add_argument("--agent-browser", default="agent-browser")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--expect-title", default="Skate 3")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    cli = [args.agent_browser, "--session", "korri-portal", "--cdp", args.cdp, "--json"]

    def run(*command: str):
        completed = subprocess.run(
            [*cli, *command], text=True, capture_output=True, timeout=40
        )
        result = json.loads(completed.stdout)
        if completed.returncode != 0 or not result.get("success"):
            raise RuntimeError(result)
        return result["data"]

    reports = []
    for surface, selector in [
        ("pico", ".pico-screen"),
        ("shift", "[data-shift-surface]"),
    ]:
        run("errors", "--clear")
        run("open", f"http://127.0.0.1:8099/?surface={surface}")
        run("wait", selector)
        facts = run(
            "eval",
            """({
          text: document.body.innerText,
          width: innerWidth, height: innerHeight,
          reducedMotion: matchMedia('(prefers-reduced-motion: reduce)').matches,
          nativeBridge: typeof window.KorriNative,
          backendRequests: performance.getEntriesByType('resource')
            .filter(entry => ['fetch', 'xmlhttprequest'].includes(entry.initiatorType))
            .map(entry => entry.name),
          preference: localStorage.getItem('korri.surface')
        })""",
        )["result"]
        assert (facts["width"], facts["height"]) == (640, 480), facts
        assert facts["reducedMotion"] is True, facts
        assert facts["nativeBridge"] == "undefined", facts
        assert facts["backendRequests"] == [], facts
        assert facts["preference"] == surface, facts
        if surface == "pico":
            assert args.expect_title in facts["text"], facts
        run("screenshot", str(args.output / f"{surface}.png"))
        refs = run("snapshot", "-i")["refs"]
        first_button = next(
            ref for ref, value in refs.items() if value["role"] == "button"
        )
        run("focus", f"@{first_button}")
        focus_name = "(document.activeElement.getAttribute('aria-label') || document.activeElement.textContent).trim()"
        before = run("eval", focus_name)["result"]
        expected_focus = "Neverball" if surface == "pico" else "Library"
        assert before != expected_focus, before
        run("press", "ArrowRight")
        run("wait", "--fn", f"{focus_name} === {json.dumps(expected_focus)}")
        assert run("eval", "document.activeElement.tagName")["result"] == "BUTTON"
        if surface == "pico":
            run("find", "role", "button", "click", "--name", "Neverball")
            text = run("get", "text", "body")["text"]
            assert "PICO ▸ GAME" in text, text
        else:
            run("find", "role", "button", "click", "--name", "Settings")
            run("press", "Enter")
            run("wait", ".shift-settings-stage, .shift-settings-empty")
        run("screenshot", str(args.output / f"{surface}-interaction.png"))
        run("press", "Escape")
        if surface == "pico":
            run("wait", "--text", "PICO ▸ LIBRARY")
        else:
            run(
                "wait",
                "--fn",
                "document.querySelector('.shift-settings-stage, .shift-settings-empty') === null",
            )
            run("wait", "--text", "Library")
        errors = run("errors")
        assert not errors.get("errors"), errors
        reports.append({"surface": surface, "facts": facts, "errors": errors})
    # The device is a Pico preview. Select it through the real portal seam,
    # then verify the bare boot URL honors its remembered preference.
    run("open", "http://127.0.0.1:8099/?surface=pico")
    run("wait", ".pico-screen")
    run("open", "http://127.0.0.1:8099/")
    run("wait", ".pico-screen")
    (args.output / "report.json").write_text(json.dumps(reports, indent=2) + "\n")
    print(json.dumps(reports, indent=2))


if __name__ == "__main__":
    main()
