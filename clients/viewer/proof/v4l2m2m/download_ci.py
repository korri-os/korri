#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#gh nixpkgs#git --command python3
"""Verify and download a signed architecture artifact from the pinned dry run.

This does not publish or sign anything. The device's normal archive importer
must still verify the bound publisher's signatures and the exact approval.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from urllib.parse import urlparse
import zipfile


def api(endpoint: str):
    return json.loads(subprocess.check_output(["gh", "api", endpoint]))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", type=int, required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--source-checkout", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--record", type=Path, required=True)
    args = parser.parse_args()
    head = subprocess.check_output(
        ["git", "-C", str(args.source_checkout), "rev-parse", "HEAD"], text=True
    ).strip()
    if head != args.source_sha:
        raise SystemExit("the source checkout is not at the tested SHA")
    base = "repos/korri-os/plugins/actions"
    run = api(f"{base}/runs/{args.run}")
    if (
        run["event"] != "workflow_dispatch"
        or run["head_branch"] != "main"
        or args.source_sha not in run["display_title"]
    ):
        raise SystemExit("the run does not name the expected pinned dry-run source")
    jobs = api(f"{base}/runs/{args.run}/jobs")["jobs"]
    arm = [
        job for job in jobs if job["name"] == "build (aarch64-linux, ubuntu-24.04-arm)"
    ]
    if len(arm) != 1 or arm[0]["conclusion"] != "success":
        raise SystemExit("the ARM build, checks and signing job did not pass")
    artifacts = api(f"{base}/runs/{args.run}/artifacts")["artifacts"]
    chosen = [
        item
        for item in artifacts
        if item["name"] == "cache-aarch64-linux" and not item["expired"]
    ]
    if len(chosen) != 1:
        raise SystemExit("there is not exactly one live ARM artifact")
    artifact = chosen[0]
    args.out.mkdir(parents=True, exist_ok=True)
    archive = args.out / "artifact.zip"
    with archive.open("wb") as stream:
        subprocess.run(
            ["gh", "api", f"{base}/artifacts/{artifact['id']}/zip"],
            stdout=stream,
            check=True,
        )
    with archive.open("rb") as stream:
        digest = "sha256:" + hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != artifact["digest"]:
        raise SystemExit("downloaded artifact differs from GitHub's recorded SHA-256")
    with zipfile.ZipFile(archive) as zipped:
        for member in zipped.infolist():
            path = Path(member.filename)
            if path.is_absolute() or ".." in path.parts:
                raise SystemExit("artifact contains an unconfined path")
        zipped.extractall(args.out / "contents")
    contents = args.out / "contents"
    if (contents / "revision.txt").read_text().strip() != args.source_sha:
        raise SystemExit("artifact revision differs from the tested SHA")
    catalog = json.loads((contents / "repository/catalog.json").read_text())
    release = catalog["latest"]["@korri:viewer"]
    records = [
        record
        for record in catalog["records"]
        if record["plugin_id"] == "@korri:viewer"
        and record["platform"] == "aarch64-linux"
        and record["release_version"] == release
    ]
    if len(records) != 1:
        raise SystemExit("catalog does not contain one selected ARM viewer")
    record = records[0]
    plugin = contents / "repository" / Path(urlparse(record["archive_url"]).path).name
    with plugin.open("rb") as stream:
        plugin_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    if plugin_hash != record["archive_sha256"]:
        raise SystemExit("the signed archive differs from the architecture catalog")
    report = {
        "run": args.run,
        "source_sha": args.source_sha,
        "artifact_id": artifact["id"],
        "artifact_sha256": digest,
        "architecture_job": arm[0]["conclusion"],
        "run_conclusion_at_download": run["conclusion"],
        "record": record,
        "local_archive": str(plugin),
    }
    args.record.write_text(json.dumps(report, indent=2) + "\n")
    print(plugin)


if __name__ == "__main__":
    main()
