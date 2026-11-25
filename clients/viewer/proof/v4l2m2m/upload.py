#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#openssh nixpkgs#git --command python3
"""Stage an existing signed CI archive. Review/install it with korri --upload.

The CLI has no prebuilt-archive input. This uses exactly its existing chunked
upload calls (clients/cli/src/plugin_sideload.rs), through its person socket.
The device still checks archive signatures, publisher binding and approval.
This script neither builds, signs, approves nor enables a plugin.
"""

import argparse
import base64
import hashlib
import http.client
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, path: Path):
        super().__init__("localhost", timeout=120)
        self.path = path

    def connect(self) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(str(self.path))


def request(path: Path, operation: str, payload: dict) -> dict:
    # RpcRequest/RpcResponse in contracts/generated/korrid.ts own these tags.
    tag = f"system.plugins.archive.{operation}"
    connection = UnixHTTP(path)
    try:
        connection.request(
            "POST",
            "/rpc",
            json.dumps({"_tag": tag, "payload": payload}),
            {"Content-Type": "application/json"},
        )
        response = connection.getresponse()
        body = response.read(1024 * 1024 + 1)
        if response.status != 200 or len(body) > 1024 * 1024:
            raise RuntimeError(
                f"{operation}: HTTP {response.status}, {len(body)} bytes"
            )
        result = json.loads(body)
        if result.get("_tag") != tag or result.get("outcome", {}).get("_tag") != "Ok":
            raise RuntimeError(json.dumps(result))
        return result["outcome"]["payload"]
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("device", choices=["tabs6", "op6"])
    parser.add_argument("archive", type=Path)
    parser.add_argument(
        "--sha256", required=True, help="Digest from the downloaded CI catalog"
    )
    parser.add_argument("--source-checkout", type=Path, required=True)
    parser.add_argument("--source-sha", required=True)
    args = parser.parse_args()
    head = subprocess.check_output(
        ["git", "-C", str(args.source_checkout), "rev-parse", "HEAD"], text=True
    ).strip()
    if head != args.source_sha:
        parser.error("the source checkout is not at the tested SHA")
    with args.archive.open("rb") as archive:
        digest = hashlib.file_digest(archive, "sha256").hexdigest()
    size = args.archive.stat().st_size
    if digest != args.sha256 or size == 0:
        parser.error(
            "the archive does not match the recorded catalog digest, or is empty"
        )
    with tempfile.TemporaryDirectory(prefix="korri-616-upload-") as temporary:
        path = Path(temporary) / "person.sock"
        process = subprocess.Popen(
            [
                "ssh",
                "-N",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=15",
                "-o",
                "ExitOnForwardFailure=yes",
                "-p",
                "2222",
                "-L",
                f"{path}:/run/korrid-cli/cli.sock",
                f"root@{args.device}.local",
            ]
        )
        try:
            deadline = time.monotonic() + 20
            while not path.exists():
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("the CLI socket tunnel did not open")
                time.sleep(0.1)
            upload = request(path, "begin", {"sha256": digest, "size": size})[
                "uploadId"
            ]
            print(
                json.dumps(
                    {
                        "device": args.device,
                        "sha256": digest,
                        "size": size,
                        "uploadId": upload,
                    }
                ),
                flush=True,
            )
            offset = 0
            with args.archive.open("rb") as archive:
                while chunk := archive.read(1024 * 1024):
                    request(
                        path,
                        "chunk",
                        {
                            "uploadId": upload,
                            "offset": offset,
                            "data": base64.b64encode(chunk).decode("ascii"),
                        },
                    )
                    offset += len(chunk)
            request(path, "finish", {"uploadId": upload})
            print(
                f"Staged {offset} bytes. No approval or installation performed.",
                flush=True,
            )
            print(
                f"ssh -p 2222 root@{args.device}.local korri --json --device {args.device} plugins sideload --upload {upload}"
            )
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == "__main__":
    main()
