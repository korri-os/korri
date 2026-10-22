#!/usr/bin/env python3
"""File-backed `gh api` release-assets contract for offline concurrency tests.

Writes use a process lock, asset bytes live in files, and duplicate POSTs fail
without replacement. A barrier gives both clients the same two initial scans.
"""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
import time
from urllib.parse import parse_qs, urlsplit


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["api"])
    parser.add_argument("--hostname", choices=["github.com"], required=True)
    parser.add_argument("--method", choices=["GET", "POST"], default="GET")
    parser.add_argument("--header", action="append", default=[])
    parser.add_argument("--input", type=Path)
    parser.add_argument("endpoint")
    args = parser.parse_args()
    root = Path(os.environ["KORRI_CACHE_TEST_API"])
    worker = os.environ["KORRI_CACHE_TEST_WORKER"]
    endpoint = urlsplit(args.endpoint)
    route = endpoint.path.removeprefix("/")
    query = parse_qs(endpoint.query)
    prefix = "repos/korri-os/nix-cache/releases"
    status = 200
    wait = False
    data: bytes | list[object] | dict[str, object]
    with (root / "lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        release = json.loads((root / "release.json").read_text())
        assets = json.loads((root / "assets.json").read_text())
        config = json.loads((root / "config.json").read_text())
        if args.method == "GET" and route == prefix:
            data = [release]
        elif args.method == "GET" and route == f"{prefix}/{release['id']}/assets":
            data = list(assets.values())
            page = int(query.get("page", ["1"])[0])
            size = int(query.get("per_page", ["100"])[0])
            data = data[(page - 1) * size : page * size]
            if config.get("barrier"):
                # assets() compares two scans before a write. Keep both
                # readers at the same initial snapshot for both scans.
                for scan in range(2):
                    marker = root / f"snapshot-{worker}-{scan}"
                    if not marker.exists():
                        marker.touch()
                        wait = scan + 1
                        break
            if (
                not wait
                and config.get("refresh_error")
                and (root / "collision").exists()
            ):
                status, data = 503, {"message": "asset refresh unavailable"}
        elif args.method == "GET" and route.startswith(prefix + "/assets/"):
            asset_id = route.rsplit("/", 1)[1]
            if args.header != ["Accept: application/octet-stream"]:
                raise ValueError("metadata downloads must request bytes")
            data = (root / "objects" / asset_id).read_bytes()
        elif args.method == "POST" and route == f"{prefix}/{release['id']}/assets":
            if endpoint.netloc != "uploads.github.com":
                raise ValueError("uploads must select the GitHub upload endpoint")
            if args.header != ["Content-Type: application/octet-stream"]:
                raise ValueError("uploads must send bytes")
            name = query["name"][0]
            if config.get("post_error"):
                status, data = config["post_error"], {"message": "upload unavailable"}
            elif name in assets:
                status, data = 422, {"message": "Validation Failed: already_exists"}
                (root / "collision").touch()
                assets[name].update(config.get("collision_asset", {}))
                release.update(config.get("collision_release", {}))
                if config.get("remove_on_collision"):
                    del assets[name]
            else:
                payload = args.input.read_bytes()
                asset_id = len(assets) + 1
                (root / "objects" / str(asset_id)).write_bytes(payload)
                data = {
                    "id": asset_id,
                    "name": name,
                    "state": "uploaded",
                    "size": len(payload),
                    "digest": "sha256:" + hashlib.sha256(payload).hexdigest(),
                }
                assets[name] = data
                status = 201
            (root / "assets.json").write_text(json.dumps(assets))
            (root / "release.json").write_text(json.dumps(release))
        else:
            status, data = 404, {"message": "Not Found"}
        with (root / "events.jsonl").open("a") as events:
            events.write(
                json.dumps(
                    {
                        "worker": worker,
                        "method": args.method,
                        "route": route,
                        "status": status,
                    }
                )
                + "\n"
            )
    if wait:
        deadline = time.monotonic() + 10
        while len(list(root.glob(f"snapshot-*-{wait - 1}"))) < 2:
            if time.monotonic() > deadline:
                raise TimeoutError("second uploader did not reach its snapshot")
            time.sleep(0.005)
    if status >= 400:
        assert isinstance(data, dict)
        sys.stderr.write(f"gh: {data['message']} (HTTP {status})\n")
        raise SystemExit(1)
    if isinstance(data, bytes):
        sys.stdout.buffer.write(data)
    else:
        print(json.dumps(data))


if __name__ == "__main__":
    main()
