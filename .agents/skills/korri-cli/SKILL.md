---
name: korri-cli
description: Operate Korri devices from a shell with the `korri` CLI. Use to read or change live korrid state (status, peers, library, play and sessions, launch settings, downloads, plugins, saves, Wi-Fi, identity, setup) on this device or a peer through `--device`, and when a script parses korri output. Reach for it before calling korrid's RPC socket directly.
---

`korri --help` and `korri <area> … --help` are the command reference. Read the help page of a command before you run it. This file holds only what help does not say.

## Get a CLI that matches korrid

- On a Korri device, use the `korri` on PATH. It ships with that device's korrid.
- From a checkout, `nix run .#korri -- …` builds the CLI from that commit. It talks correctly only to a korrid built from the same commit. A mismatch prints "Korrid returned an invalid response" or "Korrid refused the request", then "Check that korri and korrid use the same version".
- Help, `version` and `completion` need no daemon.

## Run it from an agent shell

Your shell is not a terminal, so korri never prompts:

- An ambiguous device name or game title stops and lists the matches. Run again with the exact ID or key.
- A destructive command needs `--yes`.
- A secret goes in through stdin (`--password-stdin`, `--credential-stdin`, `--private-stdin`). Pipe it from a file or a variable.

Add `--json` when you read the output. It prints korrid's `RpcResponse` values unchanged; each one has an `outcome._tag` of `Ok` or `Err`. Exit codes: 0 success, 1 failure, 2 usage, 4 local korrid unreachable. Exit 1 includes partial failure: `status` exits 1 when one of its seven sections fails, and the other six still hold good data.

After exit 1 from a mutation, read every response before you act again. `play` and plugin installs can be accepted after an earlier check failed, so a second run launches or installs a second time. A settings revision conflict is final: read the settings again, then decide.

## Decisions that stay with the user

`--yes` confirms destruction only. These choices come from the user, never from `--yes` or from you:

- the plugin approval digest (`--approval` on `plugins install` and `plugins update`) and publisher trust (`plugins trust`); show the user the `plugins review` output. A new release of an installed plugin goes through `plugins update`, not the host's `korri-plugin`
- `dataDisposition` and `trustLossConfirmed` for `identity switch`
- recovery words: `setup finish`, `identity switch words` and `join enter` on an owned device need a person at a terminal; never type, store or log the words for them
- `saves restore`, which overwrites the current saves

`identity reset` is unavailable by design. For device identity recovery, stop and ask the user.

## Other devices

`--device NAME_OR_KEY` picks the device that performs the operation. The request still goes to the local korrid, which forwards it over the signed peer transport. Saves, identity, Wi-Fi, setup and `library folders add` are local-only: run `korri` over SSH on that device. In `devices list`, `Ready` is a directory record, not proof that the peer answers.

Physical controller tests and power actions exist only in the shell.

## Deeper reference

- `clients/cli/README.md`: the JSON array shape of each command, the plugin sideload and resume flow, and the peer permission tables.
- `clients/cli/PLAY-LIBRARY.md`: launch-setting layers and runner scopes.
- To change the CLI itself, follow "Add an area" in the README and test with `nix run .#cli-check`.
