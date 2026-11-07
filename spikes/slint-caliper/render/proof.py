"""Inject two defects into a temporary Pico copy, check, then restore both."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

binary, repo, out = map(pathlib.Path, sys.argv[1:])
binary = binary.resolve()
repo = repo.resolve()
out = out.resolve()
out.mkdir(parents=True, exist_ok=True)
relative = pathlib.Path("clients/shell/packages/pico/ui/atoms/PicoButton.atom.preview.slint")
original = (repo / relative).read_text()
proofs = []
with tempfile.TemporaryDirectory(prefix="slint-render-pico-copy-") as tmp:
    package = pathlib.Path(tmp) / "pico"
    shutil.copytree(repo / "clients/shell/packages/pico", package)
    part = package / "ui/atoms/PicoButton.atom.preview.slint"
    part.chmod(0o644)
    for name, injection, check in [
        ("spill", 'Rectangle { x: parent.width - 5px; y: 0px; width: 40px; height: 30px; background: red; }', "spill"),
        ("missing-label", 'Rectangle { x: 1px; y: 1px; width: 20px; height: 20px; background: red; accessible-role: button; accessible-item-selectable: true; }', "accessibility"),
    ]:
        try:
            part.write_text(original.replace("background: P8.bg;", "background: P8.bg;\n" + injection, 1))
            call = subprocess.run([str(binary), "--part", str(part), "--state", "Default", "--device", "rg35xxsp", "--check", "--out", str(out / name)], capture_output=True, text=True, check=True)
            result = json.loads(call.stdout)
            (out / f"{name}.json").write_text(call.stdout)
            finding = next(c for c in result["results"][0]["checks"] if c["name"] == check)
            assert finding["status"] == ("Review" if name == "spill" else "Failed"), finding
            if name == "spill":
                assert any(e["right"] > 640 for e in result["results"][0]["spill"]["elements"])
            else:
                assert "no label" in finding["detail"]
            proofs.append({"injection": injection, "caught": finding, "reportPath": result["reportPath"]})
        finally:
            part.write_text(original)
        assert part.read_text() == original
assert (repo / relative).read_text() == original
(out / "mutation-proof.json").write_text(json.dumps({"part": str(relative), "originalSha256": hashlib.sha256(original.encode()).hexdigest(), "restored": True, "proofs": proofs}, indent=2))
print("PASS: deliberate spill and missing label caught in a Pico copy; both restored; original package untouched")
