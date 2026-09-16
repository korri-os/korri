# The repository-root interactive shell. It carries the commit-hook tooling and
# nothing else; per-area toolchains live in their own devshell.nix.
{
  pkgs,
  hooksInstall,
}:
pkgs.mkShell {
  name = "korri";
  packages = [
    pkgs.gitleaks
    pkgs.lefthook
  ];
  # Git refuses to read hooks out of the tree it is about to commit, so
  # something has to write them into the repository once. Entering this shell
  # is that moment, which is why a clone needs no separate install step.
  # `nix run .#hooks-install` runs the same program for anyone without direnv.
  #
  # Entering also roots the shell through direnv's own flake profile, so the
  # lefthook build behind the hook survives a garbage collection.
  shellHook = ''
    ${hooksInstall}
  '';
}
