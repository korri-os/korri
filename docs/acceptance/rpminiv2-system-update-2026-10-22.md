# Mini V2 internal-UFS system update — 2026-10-22

## Scope and source

The owner approved one Mini V2 update. The parent must release the device from
`bench` before any write. No other device, block write, partition-table change,
firmware change or backup of user data is part of this operation. Only the
on-device GRUB/profile rollback is retained.

Fetched and rebased onto `origin/main` at
`4e34b8a9ed859560acac59c218c7af872221d3ed`. A later read-only `ls-remote`
confirmed the same main tip. The factoring does not change the system path:

```text
/nix/store/hp7kgka5q3abwy25n48p0a511j6kxqjl-nixos-system-rpminiv2-sd-card-26.11.20260923.4975466
```

## Offline verification

| Check | Result |
|---|---|
| `nix run .#rpminiv2-update-check` | 8 passed; production renderer for two toplevels, new/previous order, unsafe config and mount/label refusals, low-space refusal, existing-payload protection, real filesystem and Nix-profile rollback. ShellCheck passed. |
| `nix build --no-link .#checks.x86_64-linux.rpminiv2` | Passed: 34 image-verifier tests, 2 fan-map tests, 8 kernel-config tests and 2 thermal tests, plus module assertions. |
| System + shared firmware builds | Passed on Zao/configured builders; no target build. |
| Ruff check/format; Python bytecode compilation | Passed for the new Python files. No project Python typechecker is configured. |
| Nix evaluation + formatting | New Nix expressions formatted; product/recovery module evaluation passed. |
| `nix run .#help` | Both update apps listed. |
| `nix run .#rpminiv2-update -- --status` | Real read-only path passed on Mini V2 SSH port 2222. |

### Image firmware identity

Before editing, saved the evaluated original `sdImage.populateFirmwareCommands`
from main. Ran that exact script into `/tmp/rpminiv2-firmware-before.3kjmhT/`
after its referenced paths were built. `diff -r` against the new shared output
`/nix/store/qvyl7xyvrq9gmxblxyifx1jzsdp55cj2-rpminiv2-firmware` exited 0.
All **9 filenames and file bytes** match for the same toplevel, including
GRUB, BLS, kernel, initrd, DTB, font and both loaders. The system toplevel path
also equals the pre-edit evaluated path. Whole disk-image GUIDs are already
non-deterministic; this proof compares the actual populated firmware contents,
not full-image hashes.

```text
39de9119311fa4274f27908a561f2b876133810325d9deff89a9461f832c838b  EFI/BOOT/BOOTAA64.EFI
f9de4f9f3faa4b914901eab6eeb819a2dba79b6398c709bab73df5d3ddd317e9  EFI/nixos/s1yqrmp6xk30x8h7vbywkm73jnwpsmni-initrd-linux-aarch64-unknown-linux-gnu-7.2-initrd
3692c7ea66cda7228d7755737318365252881e4d76ec9a7d5e0ae9508020aa3e  EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-Image
53abe644510292a5399bed66277e2ef558067789834084d58eeb72b400641543  EFI/nixos/zz84y81zh6kb8jkgywmk9byvq6590lnp-linux-aarch64-unknown-linux-gnu-7.2-sm8250-retroidpocket-rpminiv2.dtb
ed24e7e8b385f25f9df8812c899ecf581a6aa87368827b6a4a41fe4cb6f513cd  EFI/systemd/systemd-bootaa64.efi
734f45a5b8c134b5cc161d02a9650fa7cf939abbab1a32f254bab8da201c9385  boot/grub/dejavu-mono.pf2
505dd4b1ca61f81d2a386dde578f616c13dbba77c3fdb02e8c644903cb63ec42  boot/grub/grub.cfg
b3f4929575ff82b657d9234e3e2bda7f2964baa01b5af7c9f4c292b0399c50c0  loader/entries/nixos-generation-1.conf
a6c7f5aafd644a48e48efb3f953c08f946cbf443f40ed74e108fb6c91dc127af  loader/loader.conf
```

## Device acceptance

Pending parent go; no update write has run.
