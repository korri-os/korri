#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#nix --command python3
"""Exercise real published proofs against an unsigned, private Nix database."""

import argparse
import base64
import io
import json
import os
import re
import runpy
import subprocess
import tarfile
import tempfile
from pathlib import Path


merge = runpy.run_path(str(Path(__file__).with_name("published-plugin-proofs.py")))[
    "merge"
]


def archive(path, entries):
    with tarfile.open(path, "w:gz") as target:
        for name, data in sorted(entries.items()):
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            target.addfile(entry, io.BytesIO(data))
    return path


def reject_merge(work, label, closure, archives, message):
    try:
        merge(work / label, closure, archives)
    except ValueError as error:
        assert message in str(error), (label, str(error))
        print(f"PASS {label}: {error}", flush=True)
    else:
        raise AssertionError(f"{label}: merge accepted invalid publisher proofs")


def equivalent_proofs(work, closure, archives):
    originals = {}
    for source_archive in archives:
        with tarfile.open(source_archive) as source:
            for entry in source.getmembers():
                if entry.isfile():
                    stream = source.extractfile(entry)
                    assert stream is not None
                    originals.setdefault(entry.name.removeprefix("./"), []).append(
                        stream.read()
                    )
    good = work / "good"
    merge(good, closure, archives)
    differing = []
    for name, proofs in originals.items():
        expected = proofs[0]
        if len(set(proofs)) > 1:
            assert name.endswith(".narinfo"), name
            differing.append((name, proofs))
            signatures = {
                line
                for proof in proofs
                for line in proof.splitlines(keepends=True)
                if line.startswith(b"Sig: ")
            }
            expected = b"".join(
                line
                for line in proofs[0].splitlines(keepends=True)
                if not line.startswith(b"Sig: ")
            ) + b"".join(sorted(signatures))
        assert (good / name).read_bytes() == expected, name

    for name, proofs in differing:
        first = proofs[0]
        other = next(proof for proof in proofs if proof != first)
        # Minimize to actual overlapping release proofs. Reversing their order
        # must still union signatures, but retain only the first's Deriver.
        pair = [
            archive(work / f"{name}-{index}.tar.gz", {name: proof})
            for index, proof in enumerate([first, other])
        ]
        one_path = work / f"{name}-closure"
        one_path.write_bytes(
            next(
                line[len(b"StorePath: ") :]
                for line in first.splitlines(keepends=True)
                if line.startswith(b"StorePath: ")
            )
        )
        signatures = sorted(
            {
                line
                for proof in [first, other]
                for line in proof.splitlines(keepends=True)
                if line.startswith(b"Sig: ")
            }
        )
        for label, ordered, retained in [
            ("forward", pair, first),
            ("reverse", list(reversed(pair)), other),
        ]:
            output = work / f"{name}-{label}"
            merge(output, one_path, ordered)
            expected = b"".join(
                line
                for line in retained.splitlines(keepends=True)
                if not line.startswith(b"Sig: ")
            ) + b"".join(signatures)
            assert (output / name).read_bytes() == expected, (name, label)

        # Keep the real Deriver/Sig differences while changing each other field.
        for field in [
            b"NarHash",
            b"NarSize",
            b"References",
            b"StorePath",
            b"URL",
            b"Compression",
            b"FileHash",
            b"FileSize",
        ]:
            prefix = field + b": "
            line = next(
                line
                for line in other.splitlines(keepends=True)
                if line.startswith(prefix)
            )
            changed = other.replace(line, line.removesuffix(b"\n") + b"-conflict\n")
            label = f"{name}-conflicting-{field.decode()}"
            reject_merge(
                work,
                label,
                one_path,
                [pair[0], archive(work / f"{label}.tar.gz", {name: changed})],
                "conflicting publisher proof",
            )
        label = f"{name}-conflicting-unknown-field"
        reject_merge(
            work,
            label,
            one_path,
            [
                pair[0],
                archive(
                    work / f"{label}.tar.gz",
                    {name: other + b"UnknownField: conflict\n"},
                ),
            ],
            "conflicting publisher proof",
        )
        # Native narinfo lines end at LF, not CR. A Sig-looking fragment inside
        # another field must not become an ignored signature line.
        embedded = []
        for index, proof in enumerate([first, other]):
            line = next(
                line for line in io.BytesIO(proof).readlines()
                if line.startswith(b"URL: ")
            )
            changed = proof.replace(
                line,
                line.removesuffix(b"\n") + f"\rSig: fragment-{index}\n".encode(),
            )
            embedded.append(
                archive(work / f"{name}-embedded-{index}.tar.gz", {name: changed})
            )
        reject_merge(
            work,
            f"{name}-conflicting-embedded-prefix",
            one_path,
            embedded,
            "conflicting publisher proof",
        )
    print(
        f"PASS real-proof-merge: {len(differing)} equivalent overlapping proofs; "
        "signature union, first Deriver, and strict field conflicts",
        flush=True,
    )
    return good


def run(command, env, *, input=None, succeeds=True):
    result = subprocess.run(
        command, input=input, text=True, capture_output=True, env=env
    )
    if succeeds and result.returncode:
        raise AssertionError(f"{' '.join(command)}\n{result.stdout}\n{result.stderr}")
    return result


def verify(work, label, cache, args, paths, key, accepted):
    directory = work / label
    directory.mkdir()
    env = os.environ.copy()
    env.update(
        NIX_CONFIG="", HOME=str(directory), XDG_CACHE_HOME=str(directory / "cache")
    )
    store = f"local?state={directory / 'state'}"
    options = [
        "--store",
        store,
        "--offline",
        "--extra-experimental-features",
        "nix-command",
        "--option",
        "substituters",
        "",
        "--option",
        "builders",
        "",
        "--option",
        "max-jobs",
        "0",
        "--option",
        "fallback",
        "false",
        "--option",
        "trusted-public-keys",
        key,
        "--option",
        "extra-trusted-public-keys",
        "",
        "--option",
        "secret-key-files",
        "",
    ]
    run(
        ["nix-store", "--store", store, "--load-db"],
        env,
        input=args.registration.read_text(),
    )
    # The whole closure, not only the plugin roots, starts without signatures.
    info = json.loads(
        run(
            ["nix", *options, "path-info", "--json", "--recursive", *args.roots], env
        ).stdout
    )
    records = list(info.values()) if isinstance(info, dict) else info
    registered = (
        set(info) if isinstance(info, dict) else {record["path"] for record in records}
    )
    assert registered == set(paths), (label, registered.symmetric_difference(paths))
    assert all(not record.get("signatures", []) for record in records), label
    unsigned = run(
        [
            "nix",
            *options,
            "store",
            "verify",
            "--recursive",
            "--no-contents",
            "--sigs-needed",
            "1",
            *args.roots,
        ],
        env,
        succeeds=False,
    )
    assert unsigned.returncode != 0 and "untrusted" in unsigned.stderr, unsigned.stderr
    copied = run(
        [
            "nix",
            *options,
            "store",
            "copy-sigs",
            "--recursive",
            "--substituter",
            cache.as_uri(),
            *args.roots,
        ],
        env,
        succeeds=False,
    )
    checked = run(
        [
            "nix",
            *options,
            "store",
            "verify",
            "--recursive",
            *([] if accepted else ["--no-contents"]),
            "--sigs-needed",
            "1",
            *args.roots,
        ],
        env,
        succeeds=False,
    )
    if accepted:
        assert copied.returncode == 0, copied.stderr
        assert checked.returncode == 0, checked.stderr
    else:
        assert checked.returncode != 0 and "untrusted" in checked.stderr, checked.stderr
        untrusted = set(re.findall(r"path '([^']+)' is untrusted", checked.stderr))
        expected = {args.dependency} if label == "bad-signature" else set(paths)
        assert untrusted == expected, (
            label,
            untrusted.symmetric_difference(expected),
            checked.stderr,
        )
    print(
        f"PASS {label}: copy-sigs exit {copied.returncode}; verify exit {checked.returncode}; "
        f"{len(paths)} initially unsigned paths",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--system", required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--closure", type=Path, required=True)
    parser.add_argument("--registration", type=Path, required=True)
    parser.add_argument("--public-key", required=True)
    parser.add_argument("--roots", nargs="+", required=True)
    parser.add_argument("--archives", type=Path, nargs="+")
    args = parser.parse_args()
    paths = args.closure.read_text().splitlines()
    assert set(args.roots) <= set(paths)
    entries = {p.name: p.read_bytes() for p in args.metadata.iterdir() if p.is_file()}
    # Select a real dependency below the roots' immediate references. Mutations
    # must exercise recursive closure coverage, not only package-root proofs.
    immediate = set(args.roots)
    for root in args.roots:
        data = entries[Path(root).name.split("-", 1)[0] + ".narinfo"]
        references = next(
            line
            for line in data.decode().splitlines()
            if line.startswith("References: ")
        )
        immediate.update("/nix/store/" + ref for ref in references[12:].split())
    args.dependency = next(path for path in paths if path not in immediate)
    name = Path(args.dependency).name.split("-", 1)[0] + ".narinfo"
    data = entries[name]
    print(
        f"{args.system}: {len(args.roots)} published roots, {len(paths)} closure paths; "
        f"transitive mutation target {args.dependency}",
        flush=True,
    )
    with tempfile.TemporaryDirectory(prefix="published-proof-test-") as temporary:
        work = Path(temporary)
        original = archive(work / "original.tar.gz", entries)
        archives = args.archives or [original]
        good = equivalent_proofs(work, args.closure, archives)
        # Actual immutable release archives and the production output must agree.
        assert {p.name: p.read_bytes() for p in good.iterdir()} == entries
        merge(work / "identical-duplicate", args.closure, [*archives, original])
        print("PASS real-proof-merge and identical-duplicate", flush=True)

        missing = entries.copy()
        del missing[name]
        reject_merge(
            work,
            "missing-transitive-proof",
            args.closure,
            [archive(work / "missing.tar.gz", missing)],
            "missing signed offline metadata",
        )
        wrong = entries.copy()
        wrong[name] = data.replace(
            f"StorePath: {args.dependency}\n".encode(),
            f"StorePath: {args.roots[0]}\n".encode(),
        )
        assert wrong[name] != data
        wrong_archive = archive(work / "wrong-path.tar.gz", wrong)
        reject_merge(
            work,
            "wrong-store-path",
            args.closure,
            [wrong_archive],
            "publisher proof names a different store path",
        )
        conflicting = archive(work / "conflict.tar.gz", {name: wrong[name]})
        reject_merge(
            work,
            "conflicting-duplicate",
            args.closure,
            [*archives, conflicting],
            "conflicting publisher proof",
        )

        verify(work, "bound-key", good, args, paths, args.public_key, True)
        key_name, encoded_key = args.public_key.split(":", 1)
        damaged_key = bytearray(base64.b64decode(encoded_key, validate=True))
        damaged_key[0] ^= 1
        wrong_key = key_name + ":" + base64.b64encode(damaged_key).decode()
        verify(work, "wrong-key", good, args, paths, wrong_key, False)

        damaged = []
        count = 0
        for line in data.decode().splitlines(keepends=True):
            if line.startswith("Sig: "):
                signer, signature = line[5:].strip().split(":", 1)
                signature_bytes = bytearray(base64.b64decode(signature, validate=True))
                signature_bytes[0] ^= 1
                line = (
                    "Sig: "
                    + signer
                    + ":"
                    + base64.b64encode(signature_bytes).decode()
                    + "\n"
                )
                count += 1
            damaged.append(line)
        assert count > 0, f"no real signatures on {args.dependency}"
        bad_entries = entries.copy()
        bad_entries[name] = "".join(damaged).encode()
        bad_cache = work / "bad-cache"
        merge(
            bad_cache,
            args.closure,
            [archive(work / "bad-signature.tar.gz", bad_entries)],
        )
        verify(work, "bad-signature", bad_cache, args, paths, args.public_key, False)
    print(f"PASS {args.system}: real signed offline closure proof checks", flush=True)


if __name__ == "__main__":
    main()
