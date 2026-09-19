# Korri task definitions. Runnable apps and generated help derive from the
# same definitions so the command surface cannot drift from its documentation.
{
  pkgs,
  proseql,
  extraHelpText ? "",
}:
let
  proseqlSource = import ../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  rustToolchain = pkgs.rust-bin.stable.latest.default;

  definitions = {
    hooks-install = {
      description = "Install the Git hook that refuses a commit which stages a secret.";
      runtimeInputs = [
        pkgs.lefthook
        pkgs.nix
      ];
      script = ''
        cd "$KORRI_ROOT"
        lefthook install
        # `lefthook install` writes the absolute store path of this lefthook
        # into the generated hook. A garbage collection would delete that path,
        # and the hook then reports a missing lefthook and lets the commit
        # through. Root the build, so nothing can disarm the gate in silence.
        git_dir="$(git rev-parse --git-common-dir)"
        case "$git_dir" in
          /*) ;;
          *) git_dir="$KORRI_ROOT/$git_dir" ;;
        esac
        nix-store --realise --indirect \
          --add-root "$git_dir/lefthook-gcroot" \
          "$(dirname "$(dirname "$(command -v lefthook)")")" >/dev/null
        echo "Installed. Git shares these hooks with every worktree of this repository."
      '';
    };
    secret-scan-staged = {
      description = "Scan the staged changes for secrets; the pre-commit hook runs this.";
      runtimeInputs = [ pkgs.gitleaks ];
      script = ''
        cd "$KORRI_ROOT"
        # --redact keeps the value out of the terminal and out of any log that
        # captures it; --verbose is what names the offending file, which the
        # committer needs in order to act.
        exec gitleaks git --staged --redact --verbose --no-banner .
      '';
    };
    r36tmax-check = {
      description = "Check the R36T Max console and Korri configurations without deploying.";
      runtimeInputs = [ pkgs.nix ];
      script = ''
        cd "$KORRI_ROOT"
        exec nix build --no-link \
          .#checks.${pkgs.stdenv.hostPlatform.system}.r36tmax \
          .#checks.${pkgs.stdenv.hostPlatform.system}.r36tmax-registry \
          .#checks.${pkgs.stdenv.hostPlatform.system}.r36tmax-recovery \
          .#checks.${pkgs.stdenv.hostPlatform.system}.r36tmax-mmc
      '';
    };
    r36tmax-session-check = {
      description = "Read bounded R36T Max memory and service diagnostics over an existing SSH connection.";
      usageSuffix = " -- <ssh-target>";
      runtimeInputs = [ pkgs.openssh ];
      script = ''
        target="''${1:?usage: r36tmax-session-check <ssh-target>}"
        exec ssh -o BatchMode=yes -o ConnectTimeout=10 "$target" \
          /run/current-system/sw/bin/r36tmax-session-sanity
      '';
    };
    rpminiv2-check = {
      description = "Check the Retroid Pocket Mini V2 Korri and recovery configurations and offline image verifier.";
      runtimeInputs = [ pkgs.nix ];
      script = ''
        cd "$KORRI_ROOT"
        exec nix build --no-link .#checks.${pkgs.stdenv.hostPlatform.system}.rpminiv2
      '';
    };
    rpminiv2-initrd-check = {
      description = "Build the Mini V2 Korri early module closure and root-time modules; cross-build on x86_64.";
      runtimeInputs = [ pkgs.nix ];
      script = ''
        cd "$KORRI_ROOT"
        exec nix build --no-link .#checks.${pkgs.stdenv.hostPlatform.system}.rpminiv2-initrd-modules
      '';
    };
    rpminiv2-image-check = {
      description = "Inspect an uncompressed RP Mini V2 recovery or Korri image without mounting or writing media.";
      usageSuffix = " -- <image.img> [--profile recovery|korri]";
      runtimeInputs = [
        pkgs.python3
        pkgs.util-linux
        pkgs.e2fsprogs
        pkgs.mtools
        pkgs.dtc
        pkgs.gptfdisk
      ];
      script = ''
        exec python3 "$KORRI_ROOT/nix/devices/rpminiv2/verify-image.py" "$@"
      '';
    };
    rgds-initrd-check = {
      description = "Build the RG DS initrd module closure with strict missing-module checks; cross-build on x86_64.";
      runtimeInputs = [ pkgs.nix ];
      script = ''
        cd "$KORRI_ROOT"
        exec nix build --no-link .#checks.${pkgs.stdenv.hostPlatform.system}.rgds-initrd-modules
      '';
    };
    rgds-image-check = {
      description = "Inspect an uncompressed RG DS image and its matching U-Boot without mounting or writing media.";
      usageSuffix = " -- <image.img> <u-boot-rockchip.bin>";
      runtimeInputs = [
        pkgs.python3
        pkgs.util-linux
        pkgs.e2fsprogs
      ];
      script = ''
        exec python3 "$KORRI_ROOT/nix/devices/rgds/verify-image.py" "$@"
      '';
    };
    korri-nix-cache = {
      description = "Publish Korri's own outputs to korri-os/nix-cache as a signed Nix binary cache.";
      usageSuffix = " -- <export|prepare|upload|combine|validate> ...";
      # gh is included deliberately: upload shells out to it, and a task that
      # needs a tool the app does not provide fails at the worst moment.
      runtimeInputs = [
        pkgs.nix
        pkgs.git
        pkgs.python3
        pkgs.gh
      ];
      script = ''
        exec python3 "$KORRI_ROOT/nix/cache/github-cache.py" "$@"
      '';
    };

    korri-nix-cache-publish = {
      description = "Build what named devices install and publish the paths no cache they trust already serves.";
      usageSuffix = " -- [--dry-run] <device>...";
      runtimeInputs = [
        pkgs.nix
        pkgs.git
        pkgs.python3
        pkgs.jq
        pkgs.gh
        pkgs.curl
        pkgs.coreutils
      ];
      script = ''
        exec bash "$KORRI_ROOT/nix/cache/publish.sh" "$@"
      '';
    };

    odin2portal-cross-cache = {
      description = "Build/export or import Odin Linux cross inputs through a signed, revision-bound Nix cache.";
      usageSuffix = " -- <export|import> <directory>";
      runtimeInputs = [
        pkgs.nix
        pkgs.git
        pkgs.coreutils
      ];
      script = ''
        exec bash "$KORRI_ROOT/nix/devices/odin2portal/cross-cache.sh" "$@"
      '';
    };
    odin2portal-cross-cache-check = {
      description = "Test Odin cross-job cache transfer with real Nix stores and tiny signed fixtures.";
      runtimeInputs = [
        pkgs.nix
        pkgs.git
        pkgs.coreutils
        pkgs.python3
      ];
      script = ''
        exec python3 "$KORRI_ROOT/nix/devices/odin2portal/cross-cache.test.py" "$KORRI_ROOT" ${pkgs.path}
      '';
    };
    device-image-dist = {
      description = "Build a pinned SD image from a clean checkout and stage its checksum and source revision.";
      usageSuffix = " -- <flake-package> <output-directory>";
      runtimeInputs = [
        pkgs.nix
        pkgs.git
        pkgs.python3
        pkgs.zstd
      ];
      script = ''
        exec python3 "$KORRI_ROOT/nix/formats/image-dist.py" build "$KORRI_ROOT" "$@"
      '';
    };
    device-image-verify = {
      description = "Check staged image checksums, compression, and revision; --release also checks GitHub asset size.";
      usageSuffix = " -- <distribution-directory> <commit-sha> [--release]";
      runtimeInputs = [
        pkgs.python3
        pkgs.zstd
      ];
      script = ''
        exec python3 "$KORRI_ROOT/nix/formats/image-dist.py" verify "$@"
      '';
    };
    device-image-dist-check = {
      description = "Test image distribution and a tiny real Nix build without building a device image.";
      runtimeInputs = [
        pkgs.nix
        pkgs.python3
        pkgs.git
        pkgs.zstd
      ];
      script = ''
        cd "$KORRI_ROOT"
        nix build --no-link .#checks.${pkgs.stdenv.hostPlatform.system}.korri-image-dist
        exec python3 "$KORRI_ROOT/nix/formats/image-dist-build.test.py" \
          "$KORRI_ROOT" ${pkgs.path} ${pkgs.stdenv.hostPlatform.system}
      '';
    };
    nixos-layout-check = {
      description = "Check the shared NixOS base, SD format, device data, and WiFi provisioning without deploying.";
      runtimeInputs = [
        pkgs.nix
        pkgs.git
        pkgs.jq
        pkgs.ripgrep
        pkgs.gnused
      ];
      script = ''
        cd "$KORRI_ROOT"
        nix build --no-link \
          .#checks.${pkgs.stdenv.hostPlatform.system}.korri-base \
          .#checks.${pkgs.stdenv.hostPlatform.system}.korri-sd-card \
          .#checks.${pkgs.stdenv.hostPlatform.system}.korri-inputplumber-data \
          .#checks.${pkgs.stdenv.hostPlatform.system}.rg353m-inputplumber \
          .#checks.${pkgs.stdenv.hostPlatform.system}.odin2portal-inputplumber \
          .#checks.${pkgs.stdenv.hostPlatform.system}.rg353m-usb-gadget \
          .#checks.${pkgs.stdenv.hostPlatform.system}.rgds \
          .#checks.${pkgs.stdenv.hostPlatform.system}.rpminiv2 \
          .#checks.${pkgs.stdenv.hostPlatform.system}.rgds-inputplumber
        # Exercise impure staging on every run without real network credentials.
        if [ -z "''${KORRI_WIFI_ENV:-}" ]; then
          wifi_fixture="$(mktemp)"
          trap 'rm -f "$wifi_fixture"' EXIT
          printf '%s\n' 'WIFI_SSID=korri-layout-test' 'WIFI_PSK=korri-layout-test-only' > "$wifi_fixture"
          export KORRI_WIFI_ENV="$wifi_fixture"
        fi
        bash "$KORRI_ROOT/nix/base/wifi-check.sh"
      '';
    };





















    korrid-check = {
      description = "Run the full host, contracts, and portal check.";
      needsProseql = true;
      runtimeInputs = [ pkgs.nix ];
      env = {
        KORRI_PORTAL_BUNDLE = "${packages.portal-bundle}/bin/portal-bundle";
        # Child nix-shell helpers must use this task's locked nixpkgs, not a channel.
        NIX_PATH = "nixpkgs=${pkgs.path}";
      };
      script = ''
        exec bash "$KORRI_ROOT/services/korrid/check.sh" "$@"
      '';
    };

    inputd-check = {
      description = "Run inputd Rust, portability, package, and NixOS module checks.";
      runtimeInputs = [ pkgs.nix ];
      script = ''
        exec bash "$KORRI_ROOT/services/inputd/check.sh" "$@"
      '';
    };

    korrid-config-review = {
      description = "Explain fixed local config initialization, checkpoint load, and last-known-good retention.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.clang
        pkgs.llvmPackages.libclang
      ];
      env = {
        CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
        HOST_CC = "${pkgs.clang}/bin/clang";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      };
      usageSuffix = " -- [storage-root]";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        export KORRI_CONFIG_REVIEW_IN_SHELL=1
        exec "$KORRI_ROOT/services/korrid/config-snapshot-review.sh" "$@"
      '';
    };














    korrid-test = {
      description = "Run the korrid host test suite.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.clang
        pkgs.llvmPackages.libclang
      ];
      env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        cd "$KORRI_ROOT/services/korrid"
        exec cargo test "$@"
      '';
    };

    portal-deploy = {
      description = "Build and update the device portal without a NixOS switch; supports --rollback and --status.";
      runtimeInputs = [
        pkgs.python3
        pkgs.nix
        pkgs.openssh
      ];
      usageSuffix = " -- user@device [--ssh-config FILE] [--rollback | --status]";
      script = ''
        exec python3 "$KORRI_ROOT/clients/portal/nix/deploy.py" "$@"
      '';
    };

    portal-runtime-check = {
      description = "Test web-bundle selection and rollback with real Nix profiles and HTTP.";
      runtimeInputs = [
        pkgs.bash
        pkgs.nix
        pkgs.coreutils
        pkgs.findutils
        pkgs.diffutils
        pkgs.curl
        pkgs.python3
        pkgs.util-linux
      ];
      script = ''
        exec bash "$KORRI_ROOT/clients/portal/nix/select.test.sh"
      '';
    };

    brand-assets = {
      description = "Regenerate the PWA and Android icons from brand/*.svg.";
      runtimeInputs = [
        pkgs.bash
        pkgs.coreutils
        pkgs.imagemagick
        pkgs.python3
        pkgs.resvg
      ];
      script = ''
        bash "$KORRI_ROOT/brand/build-assets.sh"
      '';
    };

    portal-bundle = {
      description = "Build the portal and its surfaces into clients/portal/dist.";
      runtimeInputs = [
        pkgs.bun
        pkgs.coreutils
      ];
      script = ''
        cd "$KORRI_ROOT/surfaces/shift"
        bun install --frozen-lockfile --ignore-scripts
        cd "$KORRI_ROOT/clients/portal"
        bun install --frozen-lockfile --ignore-scripts
        bun run build
        test -f dist/index.html
      '';
    };

    portal-check = {
      description = "Run portal unit tests and typecheck.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/surfaces/shift"
        bun install --frozen-lockfile --ignore-scripts
        cd "$KORRI_ROOT/surfaces/pico"
        bun install --frozen-lockfile --ignore-scripts
        cd "$KORRI_ROOT/clients/portal"
        bun install --frozen-lockfile --ignore-scripts
        bun test
        bun run typecheck
      '';
    };

    shift-check = {
      description = "Run the Shift surface unit tests and typecheck.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/surfaces/shift"
        bun install --frozen-lockfile --ignore-scripts
        bun test
        bun run typecheck
      '';
    };

    pico-check = {
      description = "Run the Pico surface unit tests, gates, and typecheck.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/surfaces/pico"
        bun install --frozen-lockfile --ignore-scripts
        bun test
        bun run typecheck
      '';
    };

    portal-dev = {
      description = "Serve the portal on the local network.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/clients/portal"
        exec bun run dev "$@"
      '';
    };








  };

  exports = pkgs.lib.mapAttrs (
    _: task:
    pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList (name: value: ''export ${name}="${value}"'') (task.env or { })
    )
  ) definitions;

  makeTask =
    name: task:
    pkgs.writeShellApplication {
      inherit name;
      runtimeInputs = [
        pkgs.bash
        pkgs.coreutils
        pkgs.git
      ]
      ++ task.runtimeInputs;
      text = ''
        if [[ -n "''${KORRI_ROOT:-}" ]]; then
          korri_root="$KORRI_ROOT"
        else
          korri_root="$PWD"
          while [[ "$korri_root" != / && ! -f "$korri_root/nix/tasks.nix" ]]; do
            korri_root="$(dirname "$korri_root")"
          done
        fi
        if [[ ! -f "$korri_root/flake.nix" || ! -f "$korri_root/nix/tasks.nix" ]]; then
          echo "Korri checkout not found; run inside it or set KORRI_ROOT" >&2
          exit 1
        fi
        KORRI_ROOT="$(cd "$korri_root" && pwd -P)"
        export KORRI_ROOT
        ${pkgs.lib.optionalString (task.needsProseql or false) proseqlSource.hydrateShell}
        ${exports.${name}}
        ${task.script}
      '';
    };

  packages = pkgs.lib.mapAttrs makeTask definitions;

  helpText =
    pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList (
        name: task: "  nix run .#${name}${task.usageSuffix or ""}\n      ${task.description}"
      ) definitions
    )
    + extraHelpText;

  help = pkgs.writeShellApplication {
    name = "help";
    runtimeInputs = [ ];
    text = ''
      cat <<'EOF'
      Korri tasks (declared in nix/tasks.nix):

      ${helpText}
      EOF
    '';
  };

  toApp = package: {
    type = "app";
    program = "${package}/bin/${package.name}";
  };
in
(pkgs.lib.mapAttrs (_: package: toApp package) packages)
// {
  help = toApp help;
  default = toApp help;
}
