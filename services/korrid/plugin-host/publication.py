#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Owner-dispatched GitHub preparation. Rust owns catalog and asset contracts.

CI invokes this with its pinned Python, Nix and gh. No publication command is
provided: immutable-release settings need an operator check before publication.
"""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import quote

SYSTEMS = ("x86_64-linux", "aarch64-linux")
PLUGIN = "@korri:tailscale"


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def configuration(env):
    owner = env["GITHUB_REPOSITORY_OWNER"]
    if (
        env["GITHUB_EVENT_NAME"] != "workflow_dispatch"
        or env["GITHUB_REF"] != "refs/heads/main"
        or env["GITHUB_ACTOR"] != owner
        or env["GITHUB_TRIGGERING_ACTOR"] != owner
    ):
        raise ValueError("only the repository owner may dispatch or rerun main")
    destination = env["PLUGIN_DESTINATION"]
    tag = env["PLUGIN_TAG"]
    release = env["PLUGIN_RELEASE"]
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", destination) or any(
        part in (".", "..") for part in destination.split("/")
    ):
        raise ValueError("supply an explicit GitHub OWNER/REPOSITORY destination")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]{0,63}", tag):
        raise ValueError(
            "supply an explicit bounded release tag without path separators"
        )
    if not re.fullmatch(r"[a-f0-9]{40}", env["GITHUB_SHA"]):
        raise ValueError("source revision must be an exact commit")
    if not release:
        raise ValueError(
            "supply a plugin release label independent of upstream version"
        )
    base = f"https://github.com/{destination}/releases/download/{quote(tag, safe='')}/"
    return destination, tag, release, base


def publisher(*args):
    binary = os.environ.get("KORRI_PUBLICATION_PUBLISHER")
    if binary:
        return run(binary, *map(str, args))
    return run("nix", "run", ".#korri-publish", "--", *map(str, args))


def verify(catalog, assets, release, base):
    return publisher(
        "verify-release", catalog, assets, base, PLUGIN, release, *SYSTEMS
    ).splitlines()


def api_command(endpoint, method="GET", accept="application/vnd.github+json"):
    # Pin the credential host even if the runner has GH_HOST configured.
    return [
        "gh",
        "api",
        "--hostname",
        "github.com",
        "--method",
        method,
        "--header",
        f"Accept: {accept}",
        "--header",
        "X-GitHub-Api-Version: 2022-11-28",
        endpoint,
    ]


def api(endpoint, method="GET", payload=None, asset=None):
    command = api_command(endpoint, method)
    if payload is not None:
        command += ["--input", "-"]
    if asset is not None:
        command += [
            "--input",
            str(asset),
            "--header",
            "Content-Type: application/octet-stream",
        ]
    return json.loads(
        subprocess.check_output(
            command,
            input=json.dumps(payload) if payload is not None else None,
            text=True,
        )
    )


def pages(endpoint):
    # Construct each URL ourselves, rather than following response-supplied hosts.
    page = 1
    while True:
        records = api(f"{endpoint}?per_page=100&page={page}")
        if not isinstance(records, list) or len(records) > 100:
            raise ValueError("invalid GitHub list response")
        yield from records
        if len(records) < 100:
            return
        page += 1


def github_id(record):
    if not isinstance(record, dict):
        raise ValueError("invalid GitHub record")
    identity = record["id"]
    if type(identity) is not int or identity <= 0:
        raise ValueError("invalid GitHub identity")
    return identity


def require_unused_tag(destination, tag, created_id=None):
    for record in pages(f"repos/{destination}/releases"):
        identity = github_id(record)
        if not isinstance(record["tag_name"], str) or type(record["draft"]) is not bool:
            raise ValueError("invalid GitHub release response")
        if record["tag_name"] == tag and identity != created_id:
            raise ValueError(
                "tag already has a draft or published release; inspect it, do not overwrite"
            )


def draft_identity(record, destination, tag):
    identity = github_id(record)
    endpoint = f"repos/{destination}/releases/{identity}"
    upload = f"https://uploads.github.com/{endpoint}/assets"
    # Do not pass credentials to a returned URL until it matches the documented
    # public GitHub origin, repository and exact newly-created release identity.
    if (
        record["url"] != f"https://api.github.com/{endpoint}"
        or record["upload_url"] != upload + "{?name,label}"
        or record["tag_name"] != tag
        or record["draft"] is not True
    ):
        raise ValueError("release identity changed or release is not a draft")
    return identity, endpoint, upload


def asset_identity(record, destination):
    identity = github_id(record)
    endpoint = f"repos/{destination}/releases/assets/{identity}"
    if (
        record["url"] != f"https://api.github.com/{endpoint}"
        or record["state"] != "uploaded"
        or not isinstance(record["name"], str)
    ):
        raise ValueError("invalid GitHub asset identity")
    return record["name"], identity


def prepare_draft(destination, tag, revision, output, names, release, base):
    root = f"repos/{destination}"
    # The commits endpoint resolves both annotated and lightweight existing tags.
    # A failed read (including auth/network errors) is never evidence of absence.
    commit = api(f"{root}/commits/{quote('refs/tags/' + tag, safe='')}")
    if commit["sha"] != revision:
        raise ValueError(
            "destination tag must already point at the checked source revision"
        )
    require_unused_tag(destination, tag)
    created = api(
        f"{root}/releases",
        "POST",
        payload={
            "tag_name": tag,
            "draft": True,
            "name": tag,
            "body": f"Prepared from Korri {revision}. Do not publish until immutable releases are enabled and every asset is verified. Catalog deployment is separate.",
        },
    )
    identity, endpoint, upload = draft_identity(created, destination, tag)

    def check_draft():
        require_unused_tag(destination, tag, identity)
        if draft_identity(api(endpoint), destination, tag) != (
            identity,
            endpoint,
            upload,
        ):
            raise ValueError("GitHub returned another release identity")

    uploaded = {}

    def check_assets():
        stored = [
            asset_identity(record, destination)
            for record in pages(endpoint + "/assets")
        ]
        if len(stored) != len(uploaded) or dict(stored) != uploaded:
            raise ValueError("draft has unexpected, missing or replaced assets")

    for name in names:
        check_assets()
        check_draft()
        # POST-only: no replacement, deletion, tag write, release edit or publish.
        record = api(
            upload + "?name=" + quote(name, safe=""),
            "POST",
            asset=output / "assets" / name,
        )
        stored_name, asset_id = asset_identity(record, destination)
        if stored_name != name or asset_id in uploaded.values():
            raise ValueError("uploaded asset identity does not match")
        uploaded[name] = asset_id
        check_draft()

    check_assets()
    # Read bytes through exact asset IDs, never browser_download_url or tag lookup.
    with tempfile.TemporaryDirectory() as temporary:
        for name, asset_id in uploaded.items():
            with (Path(temporary) / name).open("xb") as target:
                subprocess.run(
                    api_command(
                        f"{root}/releases/assets/{asset_id}",
                        accept="application/octet-stream",
                    ),
                    stdout=target,
                    check=True,
                )
        verify(output / "catalog.json", Path(temporary), release, base)
    check_assets()
    check_draft()
    return identity


def main(args):
    destination, tag, release, base = configuration(os.environ)
    command, *args = args
    if command == "guard" and not args:
        return
    if command == "build" and len(args) == 2:
        system, output = args
        if system not in SYSTEMS:
            raise ValueError("unsupported publication platform")
        output = Path(output)
        output.mkdir(parents=True, exist_ok=False)
        package = run(
            "nix",
            "build",
            "--no-link",
            "--print-out-paths",
            f".#packages.{system}.korri-tailscale",
        )
        name = publisher("archive-name", package, release, system)
        record = publisher(package, release, system, base + name, output)
        with (output / "record.json").open("x") as file:
            file.write(record + "\n")
        return
    if command == "assemble" and len(args) == 2:
        artifacts, output = map(Path, args)
        output.mkdir(parents=True, exist_ok=False)
        records = [artifacts / f"plugin-{system}" / "record.json" for system in SYSTEMS]
        catalog = publisher("catalog", *records)
        catalog_path = output / "catalog.json"
        with catalog_path.open("x") as file:
            file.write(catalog + "\n")
        # Copy regular archive files; Rust checks their expected names and hashes.
        import shutil

        assets = output / "assets"
        assets.mkdir()
        for system in SYSTEMS:
            for path in (artifacts / f"plugin-{system}").glob("*.tar"):
                if path.is_symlink() or not path.is_file():
                    raise ValueError("archive must be a regular file")
                with (
                    path.open("rb") as source,
                    (assets / path.name).open("xb") as target,
                ):
                    shutil.copyfileobj(source, target)
        names = verify(catalog_path, assets, release, base)
        if set(names) != {p.name for p in assets.iterdir()}:
            raise ValueError("unexpected release assets")
        return
    if command == "draft" and len(args) == 1:
        if not os.environ.get("KORRI_PUBLICATION_PUBLISHER"):
            raise ValueError("draft requires the already-built trusted validator")
        if os.environ.get("PLUGIN_CREATE_DRAFT") != "true":
            raise ValueError("draft creation needs explicit operator approval")
        if destination != os.environ["GITHUB_REPOSITORY"]:
            raise ValueError(
                "draft writes are limited to this repository; no cross-repository credential is configured"
            )
        output = Path(args[0])
        names = verify(output / "catalog.json", output / "assets", release, base)
        identity = prepare_draft(
            destination, tag, os.environ["GITHUB_SHA"], output, names, release, base
        )
        print(
            f"Draft {identity} assets verified. This command did not publish. Operator must verify immutable-release settings, publish the draft, then deploy catalog.json separately."
        )
        return
    raise ValueError(
        "usage: publication.py guard | build SYSTEM OUTPUT | assemble ARTIFACTS OUTPUT | draft OUTPUT"
    )


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (ValueError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
