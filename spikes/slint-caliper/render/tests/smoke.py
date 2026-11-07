"""Real-process spike proofs, not a production test framework."""
import json
import pathlib
import subprocess
import sys
import tempfile

binary = str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="slint-render-proof-") as tmp:
    root = pathlib.Path(tmp)
    part = root / "Proof.preview.slint"
    baseline = root / ".caliper/baselines"

    def source(extra="", label='accessible-label: "Go";', selected="true", x="10px"):
        part.write_text('''export component Default inherits Window {
            background: #102030;
            Rectangle { x: ''' + x + '''; y: 10px; width: 50px; height: 30px;
                background: #ff0011; accessible-role: button;
                accessible-item-selectable: true;
                accessible-item-selected: ''' + selected + ';' + label + '}' + extra + '}')

    def call(*args, code=0):
        command = [binary, *map(str, args)]
        result = subprocess.run(command, capture_output=True, text=True)
        assert result.returncode == code, (command, result.returncode, result.stdout, result.stderr)
        return json.loads(result.stdout)

    def render(check=True):
        args = ["--part", part, "--state", "all", "--device", "rg35xxsp", "--out", root / "out"]
        if check:
            args += ["--check", "--baselines", baseline]
        return call(*args)

    def status(result, name):
        return next(c for c in result["results"][0]["checks"] if c["name"] == name)

    source()
    clean = render()
    assert clean["results"][0]["frame"] == "Rendered"
    assert status(clean, "spill")["status"] == "Passed"
    assert status(clean, "determinism")["status"] == "Passed"
    assert status(clean, "accessibility")["status"] == "Review"  # no false axe pass
    assert status(clean, "focus")["status"] == "Inconclusive"  # no entry JSON
    assert status(clean, "baseline")["status"] == "Review"
    # Only synthetic test images are approved. No product baseline is auto-approved.
    assert call("--approve", clean["reportPath"], "--baselines", baseline)["approved"] == 1
    assert status(render(), "baseline")["status"] == "Passed"
    source(x="630px")
    spilled = render()
    assert status(spilled, "spill")["status"] == "Review"
    assert spilled["results"][0]["spill"]["elements"]
    assert status(spilled, "focus")["status"] == "Inconclusive"  # partial visibility is shell-visible
    source(x="650px")
    assert status(render(), "focus")["status"] == "Failed"
    assert status(spilled, "baseline")["status"] == "Review"
    source(label="")
    missing = render()
    assert status(missing, "accessibility")["status"] == "Failed"
    assert "no label" in status(missing, "accessibility")["detail"]
    source(label='accessible-label: "Go";')
    part.write_text(part.read_text().replace("accessible-role: button;", ""))
    assert render()["results"][0]["frame"] == "Failed"  # Slint itself refuses selectable properties without a role
    source(extra='Rectangle { x: 80px; y: 20px; width: 40px; height: 20px; background: blue; accessible-role: button; accessible-label: "Other"; accessible-item-selectable: true; accessible-item-selected: true; }')
    assert "2 focused elements" in status(render(), "accessibility")["detail"]
    part.write_text("export component Default inherits Window { background: black; }")
    assert render()["results"][0]["frame"] == "Empty"
    call("--part", part, "--out", root / "empty", code=1)
    part.write_text("export component Default inherits Window { broken syntax }")
    broken = render()
    assert broken["results"][0]["frame"] == "Failed"
    assert status(broken, "render")["status"] == "Failed"
    call("--approve", broken["reportPath"], "--baselines", baseline, code=2)
    source()
    valid = render()
    pathlib.Path(valid["results"][0]["png"]).write_bytes(b"changed after report")
    assert "changed or differ" in call("--approve", valid["reportPath"], "--baselines", baseline, code=2)["error"]
    assert "Unknown device" in call("--part", part, "--device", "wrong", code=2)["error"]
    assert "exports no state" in call("--part", part, "--state", "wrong", code=2)["error"]
    print("PASS: native CLI renders, compile, empty, spill, focus visibility, role/label, duplicate focus, saved approval, baseline pixel difference, tamper rejection, invalid requests")
