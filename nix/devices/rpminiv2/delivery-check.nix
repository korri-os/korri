{ pkgs }:
let
  inherit (pkgs) lib;
  imageWorkflowPath = ../../../.github/workflows/device-images.yml;
  cacheWorkflowPath = ../../../.github/workflows/nix-cache.yml;
  trustScriptPath = ../../../.github/workflows/trust-korri-cache.sh;
  imageWorkflow = builtins.readFile imageWorkflowPath;
  cacheWorkflow = builtins.readFile cacheWorkflowPath;
  trustScript = builtins.readFile trustScriptPath;
  githubShaRef = "ref: " + "$" + "{{ github.sha }}";
  checkoutLines =
    text: lib.filter (lib.hasInfix "uses: actions/checkout@") (lib.splitString "\n" text);
  checkoutIsPinned =
    line:
    let
      matched = builtins.match ".*actions/checkout@([0-9a-f]+).*" line;
    in
    matched != null && builtins.stringLength (builtins.head matched) == 40;
in
assert lib.hasInfix "          - rpminiv2" imageWorkflow;
assert lib.hasInfix "rg353m|odin2portal|rgds|rpminiv2|r36tmax" imageWorkflow;
assert lib.hasInfix ''DEVICE-sd-image" "$RUNNER_TEMP/device-dist'' imageWorkflow;
assert lib.hasInfix "device: [odin2portal, rg353m, rgds, rpminiv2, r36tmax]" cacheWorkflow;
assert lib.hasInfix "packages.x86_64-linux.rpminiv2-kernel" cacheWorkflow;
assert lib.hasInfix "packages.x86_64-linux.rpminiv2-firmware" cacheWorkflow;
assert lib.hasInfix "packages.x86_64-linux.rpminiv2-rocknix-baseline" cacheWorkflow;
assert lib.hasInfix ".github/workflows/trust-korri-cache.sh" imageWorkflow;
assert lib.hasInfix ".github/workflows/trust-korri-cache.sh" cacheWorkflow;
assert lib.hasInfix githubShaRef imageWorkflow;
assert lib.hasInfix githubShaRef cacheWorkflow;
assert lib.all checkoutIsPinned (checkoutLines imageWorkflow);
assert lib.all checkoutIsPinned (checkoutLines cacheWorkflow);
assert lib.hasInfix ".#cache.url" trustScript;
assert lib.hasInfix ".#cache.publicKeys" trustScript;
assert !(lib.hasInfix "require-sigs = false" trustScript);
pkgs.runCommand "rpminiv2-delivery-check"
  {
    nativeBuildInputs = [
      pkgs.actionlint
      pkgs.shellcheck
    ];
  }
  ''
    actionlint ${imageWorkflowPath} ${cacheWorkflowPath}
    shellcheck ${trustScriptPath}
    touch "$out"
  ''
