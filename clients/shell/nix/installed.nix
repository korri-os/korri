# The surface packages from clients/shell/packages, verified at build time.
# `korri-shell install` writes each package's summary and the combined index
# into $out/cache; the device runs `korri-shell --installed`, which only reads
# them and never verifies. A package that fails verification fails the build.
{
  pkgs,
  # The verifier runs on the build machine.
  shell ? import ../package.nix { pkgs = pkgs.buildPackages; },
}:
pkgs.runCommand "korri-shell-surfaces"
  {
    packages = ../packages;
    nativeBuildInputs = [ shell ];
  }
  ''
    mkdir -p "$out/packages"
    cp -R --no-preserve=mode "$packages"/. "$out/packages/"
    # The index records each file's size and mtime. The store resets every
    # mtime to one second after the epoch, so record that value now.
    find "$out/packages" -exec touch -h -d @1 {} +
    HOME="$TMPDIR" korri-shell install --packages "$out/packages" --cache "$out/cache" | tee "$TMPDIR/install.txt"
    if grep -v ' pass$' "$TMPDIR/install.txt"; then
      echo "a surface package failed verification" >&2
      exit 1
    fi
    test -s "$out/cache/index.json"
  ''
