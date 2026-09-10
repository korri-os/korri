{
  pkgs,
  hostModule,
  hostPackage,
  tailscalePackage,
}:
let
  # Disposable local TLS, following nixpkgs nixos/tests/headscale.nix.
  certificate =
    pkgs.runCommand "plugin-tailnet-test-certificate" { nativeBuildInputs = [ pkgs.openssl ]; }
      ''
        mkdir -p "$out"
        openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
          -out "$out/cert.pem" -keyout "$out/key.pem" \
          -subj '/CN=cache' -addext 'subjectAltName=DNS:cache'
      '';
  mkPlugin = import ./builder.nix { inherit pkgs; };
  alternate = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "clock.ts" "export const name = 'clock'; export const title = 'Clock'; export const services = ['clock'];";
    plugin = _: {
      services.clock.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
      };
      ports.allowedTCPPorts = [ 443 ];
    };
  };
  dependentClock = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "dependent-clock.ts" "export const name = 'dependent-clock'; export const services = ['clock'];";
    plugin = _: {
      requires = [ alternate ];
      services.clock.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
      };
    };
  };
  updated = pkgs.runCommand "tailscale-plugin-update" { } ''
    mkdir -p "$out"
    cp ${tailscalePackage}/manifest.json "$out/manifest.json"
    sed 's/title = "Tailscale"/title = "Tailscale updated"/' ${tailscalePackage}/plugin.ts > "$out/plugin.ts"
  '';
  broken = mkPlugin {
    publisher.namespace = "@korri";
    source = pkgs.writeText "broken.ts" "export const name = 'tailscale'; export const title = 'Unhealthy candidate'; export const services = ['broken'];";
    plugin = _: {
      services.broken.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/false";
      };
      ports.allowedTCPPorts = [ 443 ];
    };
  };
  interrupted = mkPlugin {
    publisher.namespace = "@korri";
    source = pkgs.writeText "pending.ts" "export const name = 'tailscale'; export const title = 'Pending candidate'; export const services = ['pending'];";
    plugin = _: {
      services.pending.serviceConfig = {
        Type = "notify";
        ExecStart = "${pkgs.coreutils}/bin/sleep 30";
      };
      ports.allowedTCPPorts = [ 443 ];
    };
  };
  unclean = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "unclean.ts" "export const name = 'unclean'; export const services = ['unclean'];";
    plugin = _: {
      services.unclean.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
        ExecStopPost = "${pkgs.coreutils}/bin/false";
      };
      ports.allowedUDPPorts = [ 41641 ];
    };
  };
  credential = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "credential.ts" "export const name = 'credential'; export const services = ['credential'];";
    plugin = _: {
      services.credential.serviceConfig = {
        Type = "exec";
        ExecStart = "\"${pkgs.coreutils}/bin/sleep\" \\\n# a native systemd continuation comment\n \"\\x33\\x36\\x30\\x30\"";
        CapabilityBoundingSet = [
          "CAP_NET_ADMIN"
          ""
          "CAP_NET_RAW"
        ];
        DeviceAllow = [
          "/dev/net/tun rw"
          ""
        ];
        LoadCredential = "authkey:tailscale-authkey";
      };
    };
  };
  forbidden = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "forbidden.ts" "export const name = 'forbidden'; export const services = ['forbidden'];";
    plugin = _: {
      services.forbidden.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
        User = "root";
      };
    };
  };
  injections =
    map
      (
        description:
        mkPlugin {
          publisher.namespace = "@example";
          source = pkgs.writeText "injection.ts" "export const name = 'injection'; export const services = ['injection'];";
          plugin = _: {
            services.injection = {
              unitConfig.Description = description;
              serviceConfig = {
                Type = "exec";
                ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
              };
            };
          };
        }
      )
      [
        "Network daemon\n# hidden\r[Service]\rUser=root\rExecStartPre=+${pkgs.coreutils}/bin/touch /tmp/injected-command"
        "Network daemon\r[Service]\rUser=root\rExecStartPre=+${pkgs.coreutils}/bin/touch /tmp/injected-command"
      ];
  # Admission exercises the actual shipped game declarations. No emulator is
  # started by the plugin installer, and this fixture supplies no emulator.
  gameLauncher = mkPlugin {
    publisher.namespace = "@korri";
    source = ../../../plugins/retroarch/plugin.ts;
    plugin = _: { files.retroarch = "${pkgs.coreutils}/bin/true"; };
  };
  gameRuntime = mkPlugin {
    publisher.namespace = "@korri";
    source = ../../../plugins/mgba/plugin.ts;
    plugin = _: {
      files.mgba = "${pkgs.coreutils}/bin/true";
      requires = [ gameLauncher ];
    };
  };
  changedLauncher = mkPlugin {
    publisher.namespace = "@korri";
    source = pkgs.writeText "retroarch-callback.ts" ''
      ${builtins.replaceStrings [ "export function launch(" ] [ "function originalLaunch(" ] (
        builtins.readFile ../../../plugins/retroarch/plugin.ts
      )}
      export function launch(input) { throw new Error("never run during approval"); }
    '';
    plugin = _: { files.retroarch = "${pkgs.coreutils}/bin/true"; };
  };
  emptyClock = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "empty-clock.ts" "export const name = 'clock'; export const services = [];";
    plugin = _: { };
  };
  ipv6RejectAdds = pkgs.writeShellScriptBin "ip6tables" ''
    for argument in "$@"; do
      if [ "$argument" = -A ]; then exit 42; fi
    done
    exec ${pkgs.iptables}/bin/ip6tables "$@"
  '';
  empty = mkPlugin {
    publisher.namespace = "@example";
    source = pkgs.writeText "empty.ts" "export const name = 'empty'; export const services = [];";
    plugin = _: { };
  };
  # Keep a real buildable deriver on the client, but not its output. A failed
  # substitution must not run this canary, even with permissive ambient options.
  splitDependency = builtins.derivation {
    name = "plugin-split-cache-dependency";
    system = pkgs.stdenv.hostPlatform.system;
    # Treat the prebuilt shell as a source, not a derivation dependency. This
    # keeps the test deriver's closure free of the shell's compiler/bootstrap.
    builder = builtins.appendContext (builtins.unsafeDiscardStringContext "${pkgs.bash}/bin/bash") {
      ${builtins.unsafeDiscardStringContext (toString pkgs.bash)} = {
        path = true;
      };
    };
    args = [
      "-c"
      ''
        echo attempted > /tmp/korri-plugin-build-attempt
        echo upstream-dependency > "$out"
      ''
    ];
  };
  splitPayload = mkPlugin {
    publisher.namespace = "@split";
    source = pkgs.writeText "split.ts" "export const name = 'split-cache'; export const services = ['clock'];";
    plugin = _: {
      services.clock.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
      };
    };
  };
  splitPlugin = pkgs.runCommand "plugin-split-cache" { } ''
    mkdir -p "$out"
    cp ${splitPayload}/{plugin.ts,manifest.json} "$out/"
    ln -s ${splitDependency} "$out/upstream-dependency"
  '';
  impostor = mkPlugin {
    publisher.namespace = "@victim";
    source = pkgs.writeText "impostor.ts" "export const name = 'clock'; export const services = ['clock'];";
    plugin = _: {
      services.clock.serviceConfig = {
        Type = "exec";
        ExecStart = "${pkgs.coreutils}/bin/sleep 3600";
      };
    };
  };
  # A test-only signing identity. This key grants no authority outside this VM.
  key = pkgs.writeText "plugin-test-cache-key" "korri-plugin-test:XMn+6POJ5fj568Beg6v8OLo4wMcNKehDPxH+7bUrt0Svsk3i8ixelBTno9/D1z0UPghq8N+uzEcf+5dLDFa9JQ==";
  publicKey = "korri-plugin-test:r7JN4vIsXpQU56Pfw9c9FD4IavDfrsxHH/uXSwxWvSU=";
in
pkgs.testers.runNixOSTest {
  name = "korri-runtime-plugin-host";
  # The client must import the payload from the cache, not inherit every path
  # mentioned by the driver as a hidden preinstalled test dependency.
  includeTestScriptReferences = false;
  nodes = {
    cache =
      { ... }:
      {
        virtualisation.additionalPaths = [
          tailscalePackage
          alternate
          updated
          broken
          interrupted
          unclean
          empty
          emptyClock
          gameLauncher
          gameRuntime
          dependentClock
          changedLauncher
          credential
          forbidden
          splitPlugin
          impostor
          splitDependency.drvPath
        ]
        ++ injections;
        services.nix-serve = {
          enable = true;
          secretKeyFile = toString key;
        };
        services.headscale = {
          enable = true;
          address = "0.0.0.0";
          port = 8080;
          settings = {
            server_url = "https://cache";
            dns = {
              base_domain = "tailnet";
              override_local_dns = false;
            };
            derp = {
              urls = [ ];
              server = {
                enabled = true;
                region_id = 999;
                stun_listen_addr = "0.0.0.0:3478";
              };
            };
          };
        };
        services.nginx = {
          enable = true;
          virtualHosts.cache = {
            addSSL = true;
            sslCertificate = "${certificate}/cert.pem";
            sslCertificateKey = "${certificate}/key.pem";
            locations."/split/" = {
              alias = "/var/www/split/";
            };
            locations."/repositories/" = {
              alias = "/var/www/repositories/";
            };
            locations."/" = {
              proxyPass = "http://127.0.0.1:8080";
              proxyWebsockets = true;
            };
          };
        };
        security.pki.certificateFiles = [ "${certificate}/cert.pem" ];
        services.tailscale.enable = true;
        # The fixture server runs the prebuilt build-side publisher. The cold
        # machine only downloads; neither VM evaluates flakes or compiles.
        environment.systemPackages = [
          pkgs.headscale
          hostPackage
        ];
        virtualisation.writableStore = true;
        virtualisation.writableStoreUseTmpfs = false;
        virtualisation.diskSize = 8192;
        networking.firewall.allowedTCPPorts = [
          5000
          443
        ];
        networking.firewall.allowedUDPPorts = [
          41641
          3478
        ];
        virtualisation.memorySize = 1536;
      };
    machine =
      { lib, ... }:
      {
        imports = [ hostModule ];
        services.korri.pluginHost.enable = true;
        services.korri.pluginHost.package = hostPackage;
        services.korri.pluginHost.officialCatalogUrl = "https://cache/repositories/official.json";
        # Exercise configured substitution without contacting public caches
        # from the isolated VM. The production module keeps NixOS's stock cache.
        nix.settings.substituters = lib.mkForce [ "http://cache:5000" ];
        services.korri.pluginHost.publishers = {
          "@korri" = {
            inherit publicKey;
            cacheUrl = "http://cache:5000";
          };
          "@example" = {
            inherit publicKey;
            cacheUrl = "http://cache:5000";
          };
          "@split" = {
            inherit publicKey;
            cacheUrl = "https://cache/split/plugin";
          };
          "@victim" = {
            inherit publicKey;
            cacheUrl = "https://cache/split/impostor";
          };
        };
        security.pki.certificateFiles = [ "${certificate}/cert.pem" ];
        environment.systemPackages = [
          pkgs.jq
          pkgs.iptables
        ];
        virtualisation.additionalPaths = [ ipv6RejectAdds ];
        virtualisation.useNixStoreImage = true;
        virtualisation.writableStore = true;
        virtualisation.writableStoreUseTmpfs = false;
        virtualisation.diskSize = 4096;
        virtualisation.memorySize = 1536;
      };
  };
  testScript = ''
    import json
    import shlex
    start_all()
    cache.wait_for_unit("nix-serve.service")
    machine.wait_for_unit("korri-plugin-host.service")
    generation = machine.succeed("readlink -f /run/current-system").strip()
    assert machine.succeed("nix config show max-jobs").strip() == "0"
    machine.fail("test -e ${tailscalePackage}")
    machine.fail("systemctl cat tailscaled.service")
    for compiler in ["cc", "cargo", "rustc"]:
        machine.fail("command -v " + compiler)
    machine.fail("NIX_CONFIG='trusted-public-keys =\\nextra-trusted-public-keys =' korri-plugin inspect http://cache:5000 ${tailscalePackage}")
    machine.fail("test -e ${tailscalePackage}")
    machine.fail("korri-plugin inspect http://cache:5000 github:example/plugin")

    def inspect(package):
        return json.loads(machine.succeed("korri-plugin inspect http://cache:5000 " + package))

    def install(package):
        report = inspect(package)
        machine.succeed("korri-plugin install http://cache:5000 " + package + " " + report["approval"])
        return report

    def plugin_file(report, name):
        manifest = json.loads(machine.succeed("cat " + report["package"] + "/manifest.json"))
        return manifest["files"][name]

    def assert_ports(report, enabled):
        marker = report["unit"].removesuffix(".service")
        expected = []
        if enabled:
            for protocol, field in [("tcp", "allowedTCPPorts"), ("udp", "allowedUDPPorts")]:
                expected.extend((protocol, str(port)) for port in report["ports"][field])
        for tool in ["iptables", "ip6tables"]:
            rules = machine.succeed(tool + " -w -S").splitlines()
            owned = [r for r in rules if r.startswith("-A korri-plugins ") and marker in r]
            assert len(owned) == len(expected), (tool, owned, expected)
            for protocol, port in expected:
                assert sum("-p " + protocol + " " in r and "--dport " + port + " " in r for r in owned) == 1, owned
            if enabled:
                input_rules = [r for r in rules if r.startswith("-A INPUT ")]
                assert input_rules.index("-A INPUT -j korri-plugins") < input_rules.index("-A INPUT -j nixos-fw"), input_rules
                assert input_rules.count("-A INPUT -j korri-plugins") == 1, input_rules

    empty = install("${empty}")
    machine.succeed("korri-plugin enable @example:empty")
    machine.succeed("korri-plugin restore-all")
    machine.fail("test -e /run/systemd/system/" + empty["unit"])
    machine.succeed("korri-plugin disable @example:empty")
    machine.succeed("korri-plugin remove @example:empty --purge")

    # A cached closure is not activation approval. Install in dependency order,
    # then activate explicitly. The enabled listing rechecks exact source bytes,
    # approvals and the full bound publisher key without executing a callback.
    game_runtime = inspect("${gameRuntime}")
    error = machine.fail("korri-plugin install http://cache:5000 ${gameRuntime} " + game_runtime["approval"] + " 2>&1")
    assert "requires exact installed plugin" in error
    game_launcher = install("${gameLauncher}")
    install("${gameRuntime}")
    assert json.loads(machine.succeed("korri-plugin enabled-packages")) == []
    machine.fail("korri-plugin enable @korri:mgba")
    machine.succeed("korri-plugin enable @korri:retroarch")
    machine.succeed("korri-plugin enable @korri:mgba")
    game_packages = json.loads(machine.succeed("korri-plugin enabled-packages"))
    assert [p["id"] for p in game_packages] == ["@korri:mgba", "@korri:retroarch"]
    assert game_packages[0]["requires"] == ["${gameLauncher}"]
    assert game_packages[0]["declaration"]["runtimes"] == game_runtime["declaration"]["runtimes"]
    assert game_packages[1]["declaration"]["sessionControls"] == game_launcher["declaration"]["sessionControls"]
    assert all(p["native_unit"] is None for p in game_packages)
    cache.succeed("systemctl stop nix-serve.service")
    assert json.loads(machine.succeed("korri-plugin enabled-packages")) == game_packages
    cache.succeed("systemctl start nix-serve.service")
    game_root = "/nix/var/nix/gcroots/korri-plugin-host/" + game_launcher["unit"].removesuffix(".service")
    machine.succeed("ln -s ${changedLauncher} " + game_root + "/pending")
    assert "unfinished selection" in machine.fail("korri-plugin enabled-packages 2>&1")
    machine.succeed("korri-plugin restore-all")
    machine.fail("test -L " + game_root + "/pending")
    assert json.loads(machine.succeed("korri-plugin enabled-packages")) == game_packages
    machine.succeed("ln -sfn ${changedLauncher} " + game_root + "/active")
    assert "inconsistent active root" in machine.fail("korri-plugin enabled-packages 2>&1")
    machine.succeed("ln -sfn ${gameLauncher} " + game_root + "/active")
    game_receipt_path = "/var/lib/korri-plugin-host/" + game_launcher["unit"].removesuffix(".service") + "/selection.json"
    game_receipt = json.loads(machine.succeed("cat " + game_receipt_path))
    machine.succeed("printf %s " + shlex.quote(json.dumps(dict(game_receipt, approval="0" * 64))) + " > " + game_receipt_path)
    assert "no longer matches its approval" in machine.fail("korri-plugin enabled-packages 2>&1")
    machine.succeed("printf %s " + shlex.quote(json.dumps(game_receipt)) + " > " + game_receipt_path)
    for p in game_packages:
        machine.fail("test -e /run/systemd/system/" + p["unit"])
    machine.fail("korri-plugin disable @korri:retroarch")
    machine.fail("korri-plugin remove @korri:retroarch")
    changed_launcher = inspect("${changedLauncher}")
    assert changed_launcher["approval"] != game_launcher["approval"]
    machine.fail("korri-plugin update @korri:retroarch http://cache:5000 ${changedLauncher} " + game_launcher["approval"])
    machine.fail("korri-plugin update @korri:retroarch http://cache:5000 ${changedLauncher} " + changed_launcher["approval"])
    machine.succeed("korri-plugin disable @korri:mgba")
    machine.succeed("korri-plugin disable @korri:retroarch")
    # Even a disabled core retains its exact dependency selection.
    machine.fail("korri-plugin remove @korri:retroarch")
    machine.succeed("korri-plugin remove @korri:mgba --purge")
    machine.succeed("korri-plugin update @korri:retroarch http://cache:5000 ${changedLauncher} " + changed_launcher["approval"])
    # Rollback reuses exact approval, but it cannot strand even a disabled
    # runtime on a different launcher build. Closure presence is not selection.
    machine.succeed("korri-plugin restore @korri:retroarch")
    rollback_launcher = json.loads(machine.succeed("korri-plugin status @korri:retroarch"))
    assert rollback_launcher["package"] == game_launcher["package"]
    assert rollback_launcher["previous"]["package"] == changed_launcher["package"]
    assert rollback_launcher["desired"] == {"state": "Disabled"}
    install("${gameRuntime}")
    for enabled in [False, True]:
        if enabled:
            machine.succeed("korri-plugin enable @korri:retroarch; korri-plugin enable @korri:mgba")
        before_swap = json.loads(machine.succeed("korri-plugin status @korri:retroarch"))
        assert "@korri:mgba" in machine.fail("korri-plugin restore @korri:retroarch 2>&1")
        assert json.loads(machine.succeed("korri-plugin status @korri:retroarch")) == before_swap
    machine.succeed("korri-plugin disable @korri:mgba; korri-plugin remove @korri:mgba")
    machine.succeed("korri-plugin disable @korri:retroarch; korri-plugin restore @korri:retroarch")
    machine.succeed("korri-plugin enable @korri:retroarch")
    assert len(json.loads(machine.succeed("korri-plugin enabled-packages"))) == 1
    machine.succeed("korri-plugin remove @korri:retroarch --purge")

    # Load, but never start, the actual hostile bytes in systemd. Bare CR is a
    # directive boundary there, even after a comment or inside Description.
    for package in ${builtins.toJSON (map toString injections)}:
        native = json.loads(cache.succeed("cat " + package + "/manifest.json"))["services"]["injection"]
        cache.succeed("cp " + native + " /run/systemd/system/cr-injection.service; systemctl daemon-reload")
        assert cache.succeed("systemctl show cr-injection.service --property=User --value").strip() == "root"
        assert "/tmp/injected-command" in cache.succeed("systemctl show cr-injection.service --property=ExecStartPre --value")
        cache.fail("test -e /tmp/injected-command")
        cache.succeed("rm /run/systemd/system/cr-injection.service; systemctl daemon-reload")
        assert "bare carriage return" in machine.fail("korri-plugin inspect http://cache:5000 " + package + " 2>&1")
        machine.fail("korri-plugin status @example:injection")
        machine.fail("test -e /tmp/injected-command")

    assert "User" in machine.fail("korri-plugin inspect http://cache:5000 ${forbidden} 2>&1")
    machine.fail("korri-plugin status @example:forbidden")
    credential = install("${credential}")
    assert "missing named credential is non-fatal" in credential["warning"]
    machine.succeed("korri-plugin enable @example:credential")
    credential_file = "/run/credentials/" + credential["unit"] + "/authkey"
    credential_pid = machine.succeed("systemctl show " + credential["unit"] + " --property=MainPID --value").strip()
    assert machine.succeed("tr '\\0' '\\n' < /proc/" + credential_pid + "/cmdline").splitlines() == ["${pkgs.coreutils}/bin/sleep", "3600"]
    machine.fail("test -e " + credential_file)
    for property in ["CapabilityBoundingSet", "AmbientCapabilities"]:
        assert machine.succeed("systemctl show " + credential["unit"] + " --property=" + property + " --value").strip() == "cap_net_raw"
    assert "/dev/net/tun" not in machine.succeed("systemctl show " + credential["unit"] + " --property=DeviceAllow --value")
    machine.succeed("korri-plugin disable @example:credential")
    # Native systemd credential lookup only. This is deliberately not a login
    # credential and no host code reads it or runs tailscale up.
    machine.succeed("install -d -m 0700 /run/credstore; printf test-credential > /run/credstore/tailscale-authkey; chmod 0600 /run/credstore/tailscale-authkey")
    machine.succeed("korri-plugin enable @example:credential")
    machine.succeed("grep -Fx test-credential " + credential_file)
    machine.succeed("korri-plugin remove @example:credential --purge")
    machine.fail("test -e " + credential_file)
    machine.succeed("rm /run/credstore/tailscale-authkey")

    # Two real signed caches: the plugin cache has no dependency NAR or
    # narinfo; only the independently signed upstream can supply that output.
    cache.wait_for_unit("nginx.service")
    cache.succeed("mkdir -p /var/www/split/plugin /var/www/split/upstream /var/www/split/missing")
    cache.succeed("nix-store --generate-binary-cache-key split-upstream /tmp/upstream.key /tmp/upstream.pub")
    upstream_key = cache.succeed("cat /tmp/upstream.pub").strip()
    cache.succeed("nix --extra-experimental-features nix-command copy --to 'file:///var/www/split/plugin?secret-key=${key}' ${splitPlugin}")
    cache.succeed("nix --extra-experimental-features nix-command copy --to 'file:///var/www/split/upstream?secret-key=/tmp/upstream.key' ${splitDependency}")
    dependency_hash = "${builtins.substring 0 32 (builtins.baseNameOf splitDependency)}"
    dependency_info = "/var/www/split/plugin/" + dependency_hash + ".narinfo"
    dependency_nar = cache.succeed("sed -n 's/^URL: //p' " + dependency_info).strip()
    cache.succeed("rm " + dependency_info + " /var/www/split/plugin/" + dependency_nar)
    cache.succeed("cp /var/www/split/plugin/nix-cache-info /var/www/split/missing/")
    plugin_cache = "https://cache/split/plugin"
    upstream_cache = "https://cache/split/upstream"
    # Copy only the deriver at runtime: VM closure injection of a .drv also
    # includes its output and would invalidate the cold-cache test.
    machine.succeed("nix --extra-experimental-features nix-command copy --from http://cache:5000 ${splitDependency.drvPath}")
    machine.fail("test -e ${splitPlugin}")
    machine.fail("test -e ${splitDependency}")
    machine.succeed("test -e ${splitDependency.drvPath}")

    def split_command(upstream, trusted, command):
        # Deliberately weaken ambient policy. The importer must force signature
        # checks and disable local, remote and fallback builds itself.
        config = "substituters = " + upstream + "\ntrusted-public-keys = ${publicKey} " + trusted + "\nextra-trusted-public-keys =\nmax-jobs = 1\nbuilders = ssh://cache\nfallback = true\nrequire-sigs = false\nsandbox = false\nnarinfo-cache-negative-ttl = 0\n"
        return "NIX_CONFIG=" + shlex.quote(config) + " korri-plugin " + command

    # A second signer is trusted for dependencies, not for @victim. Nix accepts
    # its output into the store; both cold and cached host inspection must fail.
    cache.succeed("mkdir -p /var/www/split/impostor")
    cache.succeed("nix --extra-experimental-features nix-command copy --to 'file:///var/www/split/impostor?secret-key=/tmp/upstream.key' ${impostor}")
    impersonation = "inspect https://cache/split/impostor ${impostor}"
    machine.fail("test -e ${impostor}")
    for attempt in range(2):
        rejected = machine.fail(split_command(upstream_cache, upstream_key, impersonation) + " 2>&1")
        assert "full key bound to publisher @victim" in rejected, rejected
        machine.succeed("test -e ${impostor}")
        machine.fail("korri-plugin status @victim:clock")

    split_inspect = "inspect " + plugin_cache + " ${splitPlugin}"
    for upstream, trusted in [("https://cache/split/missing", upstream_key), (upstream_cache, "")]:
        # Also request the known buildable output directly. Refusal must not
        # depend on the plugin's own deriver being unavailable on the client.
        for command in ["inspect " + plugin_cache + " ${splitDependency}", split_inspect]:
            error = machine.fail(split_command(upstream, trusted, command) + " 2>&1")
            assert "building '/nix/store/" not in error, error
            machine.fail("test -e /tmp/korri-plugin-build-attempt")
            machine.fail("test -e ${splitDependency}")
            machine.fail("korri-plugin status @split:split-cache")
    split_report = json.loads(machine.succeed(split_command(upstream_cache, upstream_key, split_inspect)))
    assert split_report["package"] == "${splitPlugin}"
    assert split_report["provenance"] == {"kind": "RawCache", "cache_url": plugin_cache}
    machine.succeed("grep -Fx upstream-dependency ${splitPlugin}/upstream-dependency")
    machine.fail("test -e /tmp/korri-plugin-build-attempt")
    # Verification must still reject an already imported closure if its
    # dependency's signing key is no longer trusted (realization can skip it).
    machine.fail(split_command(upstream_cache, "", split_inspect))
    machine.fail("korri-plugin status @split:split-cache")
    machine.fail(split_command(upstream_cache, upstream_key, "install " + plugin_cache + " ${splitPlugin} wrong-approval"))
    machine.succeed(split_command(upstream_cache, upstream_key, "install " + plugin_cache + " ${splitPlugin} " + split_report["approval"]))
    split_receipt = json.loads(machine.succeed("korri-plugin status @split:split-cache"))
    assert split_receipt["provenance"] == split_report["provenance"]
    machine.succeed("korri-plugin enable @split:split-cache")
    machine.wait_for_unit(split_report["unit"])
    machine.succeed("korri-plugin remove @split:split-cache --purge")
    # NixOS store images can contain paths without locally registered cache
    # signatures. Verification must also consult the configured upstream when
    # realization reuses such a path rather than downloading it again.
    machine.succeed("nix-store --delete ${splitPlugin} ${splitDependency}")
    cache.succeed("cp -r /var/www/split/upstream /var/www/split/unsigned")
    cache.succeed("sed -i '/^Sig:/d' /var/www/split/unsigned/" + dependency_hash + ".narinfo")
    machine.succeed("nix --extra-experimental-features nix-command copy --no-check-sigs --from https://cache/split/unsigned ${splitDependency}")
    dependency_info = json.loads(machine.succeed("nix --extra-experimental-features nix-command path-info --json ${splitDependency}"))
    assert not dependency_info["${splitDependency}"].get("signatures", [])
    machine.fail(split_command(upstream_cache, "", split_inspect))
    reused = json.loads(machine.succeed(split_command(upstream_cache, upstream_key, split_inspect)))
    assert reused["approval"] == split_report["approval"]
    machine.fail("test -e /tmp/korri-plugin-build-attempt")

    source_a = "https://cache/repositories/a.json"
    source_b = "https://cache/repositories/b.json"
    cache.succeed("mkdir -p /var/www/repositories")

    def write_catalog(name, records):
        cache.succeed("printf %s " + shlex.quote(json.dumps({"records": records})) + " > /var/www/repositories/" + name + ".json")

    records = []
    for release, package in [("v1", "${tailscalePackage}"), ("v2", "${updated}"), ("broken", "${broken}"), ("pending", "${interrupted}")]:
        output = "/var/lib/publication-" + release
        cache.succeed("mkdir -p " + output)
        record = json.loads(cache.succeed("korri-publish " + package + " " + release + " ${pkgs.stdenv.hostPlatform.system} https://cache/repositories/" + release + ".tar " + output))
        cache.succeed("cp " + output + "/*.tar /var/www/repositories/" + release + ".tar")
        records.append(record)
    write_catalog("a", records)
    write_catalog("b", records)
    write_catalog("official", [])
    cache.wait_for_unit("nginx.service")
    machine.succeed("korri-plugin repository list | grep -F 'official: https://cache/repositories/official.json'")
    machine.fail("korri-plugin repository remove https://cache/repositories/official.json")
    machine.succeed("mv /etc/korri-plugin-host/official-catalog-url /etc/korri-plugin-host/official-catalog-url.saved")
    machine.succeed("korri-plugin repository list | grep -F 'official: not configured'")
    machine.succeed("korri-plugin restore-all")
    machine.succeed("mv /etc/korri-plugin-host/official-catalog-url.saved /etc/korri-plugin-host/official-catalog-url")
    machine.succeed("korri-plugin repository add " + source_a)
    machine.succeed("korri-plugin repository add " + source_a)
    machine.succeed("korri-plugin repository add " + source_b)
    listed = machine.succeed("korri-plugin repository list")
    assert listed.count("user-added: " + source_a) == 1
    assert "user-added: " + source_b in listed
    # The repository writer takes the same lock as installation and recovery.
    machine.succeed("systemd-run --unit=hold-plugin-lock /run/current-system/sw/bin/flock /var/lib/korri-plugin-host/lock /run/current-system/sw/bin/sleep infinity")
    try:
        machine.wait_until_succeeds("! /run/current-system/sw/bin/flock -n /var/lib/korri-plugin-host/lock /run/current-system/sw/bin/true", timeout=10)
        error = machine.fail("korri-plugin repository remove " + source_a + " 2>&1")
        assert "another plugin operation is in progress" in error, error
    finally:
        machine.succeed("systemctl stop hold-plugin-lock.service")

    def repository_inspect(source, release):
        return json.loads(machine.succeed("korri-plugin repository inspect " + source + " @korri:tailscale " + release))

    machine.fail("test -e " + records[0]["store_path"])
    report = repository_inspect(source_a, "v1")
    alternative = repository_inspect(source_b, "v1")
    assert report["package"] == alternative["package"]
    assert report["approval"] != alternative["approval"]
    assert "HOST NETWORK ADMINISTRATION" in report["warning"]
    machine.fail("korri-plugin repository install " + source_a + " @korri:tailscale v1 wrong-approval")
    machine.fail("korri-plugin repository install " + source_b + " @korri:tailscale v1 " + report["approval"])
    machine.fail("korri-plugin status @korri:tailscale")
    # A changed release between inspection and installation invalidates approval.
    changed = dict(records[1], release_version="v1")
    write_catalog("a", [changed])
    machine.fail("korri-plugin repository install " + source_a + " @korri:tailscale v1 " + report["approval"])
    # Identity, platform and archive hashes are checked before selection.
    write_catalog("a", [dict(records[0], archive_sha256="0" * 64)])
    assert "SHA256" in machine.fail("korri-plugin repository inspect " + source_a + " @korri:tailscale v1 2>&1")
    write_catalog("a", [dict(records[0], plugin_id="@example:wrong")])
    machine.fail("korri-plugin repository inspect " + source_a + " @example:wrong v1")
    write_catalog("a", [dict(records[0], platform="unsupported-linux")])
    machine.fail("korri-plugin repository inspect " + source_a + " @korri:tailscale v1")
    write_catalog("a", records)
    machine.succeed("korri-plugin repository install " + source_a + " @korri:tailscale v1 " + report["approval"])
    unit = report["unit"]
    machine.fail("systemctl is-active " + unit)
    ipv4_rules = json.loads(machine.succeed("ip -j rule show"))
    ipv6_rules = json.loads(machine.succeed("ip -6 -j rule show"))
    machine.succeed("korri-plugin enable @korri:tailscale")
    machine.wait_for_unit(unit)
    machine.succeed("ip link show tailscale0")
    machine.succeed("test -S " + report["runtime_directory"] + "/tailscaled.sock")
    assert machine.succeed("systemctl show " + unit + " --property=DynamicUser --value").strip() == "yes"
    assert machine.succeed(plugin_file(report, "tailscale") + " --socket=" + report["runtime_directory"] + "/tailscaled.sock status --json | ${pkgs.jq}/bin/jq -r .BackendState").strip() == "NeedsLogin"
    assert machine.succeed("cat /run/systemd/system/" + unit) == report["native_unit"]["source"]
    for property, expected in [("NoNewPrivileges", "yes"), ("ProtectSystem", "strict"), ("DevicePolicy", "closed"), ("CapabilityBoundingSet", "cap_net_admin cap_net_raw"), ("AmbientCapabilities", "cap_net_admin cap_net_raw")]:
        assert machine.succeed("systemctl show " + unit + " --property=" + property + " --value").strip() == expected
    machine.succeed("systemctl cat " + unit + " | grep -Fx 'DeviceAllow='")
    machine.succeed("systemctl cat " + unit + " | grep -Fx 'CapabilityBoundingSet='")
    assert_ports(report, True)
    machine.succeed("systemctl restart firewall.service")
    assert_ports(report, True)
    machine.succeed("touch " + report["state_directory"] + "/retained-data")

    # A disposable local tailnet uses no owner's credentials or public service.
    cache.wait_for_unit("headscale.service")
    cache.succeed("headscale users create plugin-test")
    authkey = cache.succeed("headscale preauthkeys -u 1 create --reusable").strip()
    cache.wait_for_unit("nginx.service")
    cache.succeed("tailscale up --login-server=https://cache --accept-dns=false --auth-key=" + shlex.quote(authkey))
    ts = plugin_file(report, "tailscale") + " --socket=" + report["runtime_directory"] + "/tailscaled.sock"
    machine.succeed(ts + " up --login-server=https://cache --accept-dns=false --auth-key=" + shlex.quote(authkey))
    machine.wait_until_succeeds(ts + " ping --timeout=5s --c=1 cache")
    assert json.loads(machine.succeed(ts + " status --json"))["BackendState"] == "Running"
    peer_ip = cache.succeed("tailscale ip -4").strip()
    machine.wait_until_succeeds("ping -c 1 -W 2 " + peer_ip)

    clock = install("${alternate}")
    # A real IPv6 helper fails only rule insertion, after IPv4 succeeded.
    # The failed enable must remove both families and leave the receipt disabled.
    failing_command = "env KORRI_PLUGIN_NIX=${pkgs.nix}/bin/nix KORRI_PLUGIN_SYSTEMCTL=${pkgs.systemd}/bin/systemctl KORRI_PLUGIN_IPTABLES=${pkgs.iptables}/bin/iptables KORRI_PLUGIN_IP6TABLES=${ipv6RejectAdds}/bin/ip6tables ${hostPackage}/bin/.korri-plugin-wrapped enable @example:clock"
    machine.fail(failing_command)
    assert_ports(clock, False)
    assert_ports(report, True)
    assert json.loads(machine.succeed("korri-plugin status @example:clock"))["desired"] == {"state": "Disabled"}
    machine.fail("systemctl is-active " + clock["unit"])
    machine.succeed("korri-plugin enable @example:clock")
    assert_ports(clock, True)
    pid = machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip()
    dependent_clock = install("${dependentClock}")
    machine.succeed("korri-plugin enable @example:dependent-clock")
    dependent_pid = machine.succeed("systemctl show " + dependent_clock["unit"] + " --property=MainPID --value").strip()
    assert int(dependent_pid) > 0
    # A corrupt unrelated receipt must not stop either dependency-free services
    # or a healthy A -> B chain. Restore reports the bad receipt independently.
    corrupt = "/var/lib/korri-plugin-host/korri-plugin-" + "0" * 64
    machine.succeed("mkdir -m 700 " + corrupt + "; (umask 077; printf broken > " + corrupt + "/selection.json)")
    assert "invalid plugin receipt" in machine.fail("korri-plugin restore-all 2>&1")
    assert machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip() == pid
    assert machine.succeed("systemctl show " + dependent_clock["unit"] + " --property=MainPID --value").strip() == dependent_pid
    machine.succeed("systemctl is-active " + unit)
    # Interrupt B's selection while A remains enabled. One restore must recover
    # B before considering A; unrelated C must still be reported separately.
    clock_root = "/nix/var/nix/gcroots/korri-plugin-host/" + clock["unit"].removesuffix(".service")
    machine.succeed("ln -s ${alternate} " + clock_root + "/pending; systemctl stop " + clock["unit"])
    assert "invalid plugin receipt" in machine.fail("korri-plugin restore-all 2>&1")
    machine.succeed("systemctl is-active " + clock["unit"])
    machine.fail("test -L " + clock_root + "/pending")
    assert machine.succeed("readlink " + clock_root + "/active").strip() == "${alternate}"
    assert machine.succeed("systemctl show " + dependent_clock["unit"] + " --property=MainPID --value").strip() == dependent_pid
    machine.succeed("rm " + corrupt + "/selection.json; rmdir " + corrupt)
    assert dependent_clock["id"] in [p["id"] for p in json.loads(machine.succeed("korri-plugin enabled-packages"))]
    # Corrupt required B instead: stop A, preserve its pins, and refuse registry
    # authority. Repairing B permits both selections to recover in one pass.
    clock_receipt_path = "/var/lib/korri-plugin-host/" + clock["unit"].removesuffix(".service") + "/selection.json"
    clock_receipt = machine.succeed("cat " + clock_receipt_path)
    machine.succeed("printf broken > " + clock_receipt_path)
    assert "invalid plugin receipt" in machine.fail("korri-plugin restore-all 2>&1")
    machine.fail("systemctl is-active " + dependent_clock["unit"])
    machine.succeed("test -L /nix/var/nix/gcroots/korri-plugin-host/" + dependent_clock["unit"].removesuffix(".service") + "/active")
    machine.fail("korri-plugin enabled-packages")
    machine.succeed("printf %s " + shlex.quote(clock_receipt) + " > " + clock_receipt_path)
    machine.succeed("korri-plugin restore-all")
    machine.succeed("systemctl is-active " + clock["unit"])
    machine.succeed("systemctl is-active " + dependent_clock["unit"])
    machine.succeed("korri-plugin remove @example:dependent-clock --purge")
    pid = machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip()
    update = repository_inspect(source_a, "v2")
    available_alternative = repository_inspect(source_b, "v2")
    machine.succeed("korri-plugin repository remove " + source_a)
    machine.fail("korri-plugin repository update @korri:tailscale v2 " + available_alternative["approval"])
    machine.succeed("korri-plugin repository add " + source_a)
    machine.fail("korri-plugin repository update @korri:tailscale v2 " + available_alternative["approval"])
    machine.succeed("korri-plugin repository update @korri:tailscale v2 " + update["approval"])
    selected = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    assert selected["provenance"]["source_url"] == source_a
    machine.fail("korri-plugin repository switch @korri:tailscale " + source_b + " v2 " + update["approval"])
    machine.succeed("korri-plugin repository switch @korri:tailscale " + source_b + " v2 " + available_alternative["approval"])
    assert json.loads(machine.succeed("korri-plugin status @korri:tailscale"))["provenance"]["source_url"] == source_b
    machine.succeed("korri-plugin repository switch @korri:tailscale " + source_a + " v2 " + update["approval"])
    ts = plugin_file(update, "tailscale") + " --socket=" + update["runtime_directory"] + "/tailscaled.sock"
    machine.wait_until_succeeds(ts + " ping --timeout=5s --c=1 cache")
    assert machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip() == pid
    assert_ports(update, True)
    before_failed_update = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    lifecycle_root = "/nix/var/nix/gcroots/korri-plugin-host/" + unit.removesuffix(".service")
    assert machine.succeed("readlink " + lifecycle_root + "/previous").strip() == before_failed_update["previous"]["package"]
    candidate = repository_inspect(source_b, "broken")
    machine.fail("korri-plugin repository switch @korri:tailscale " + source_b + " broken " + candidate["approval"])
    assert json.loads(machine.succeed("korri-plugin status @korri:tailscale")) == before_failed_update
    machine.wait_for_unit(unit)
    assert_ports(update, True)
    assert_ports(clock, True)
    restored = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    assert restored["package"] == update["package"]
    assert restored["provenance"] == update["provenance"]

    # Retain an approved but non-starting build while disabled, then prove a
    # failed explicit restore preserves both selections and enabled intent.
    machine.succeed("korri-plugin disable @korri:tailscale")
    machine.succeed("korri-plugin repository switch @korri:tailscale " + source_b + " broken " + candidate["approval"])
    machine.succeed("korri-plugin restore @korri:tailscale; korri-plugin enable @korri:tailscale")
    before_failed_update = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    assert before_failed_update["previous"]["package"] == candidate["package"]
    machine.fail("korri-plugin restore @korri:tailscale")
    assert json.loads(machine.succeed("korri-plugin status @korri:tailscale")) == before_failed_update
    machine.wait_for_unit(unit)
    assert_ports(update, True)

    pending = repository_inspect(source_b, "pending")
    machine.succeed("systemd-run --unit=interrupted-plugin-update /run/current-system/sw/bin/korri-plugin repository switch @korri:tailscale " + source_b + " pending " + pending["approval"])
    machine.wait_until_succeeds("grep -Fx 'Type=notify' /run/systemd/system/" + unit)
    machine.wait_until_succeeds("grep -F 'sleep 30' /run/systemd/system/" + unit)
    machine.wait_until_succeeds("test $(systemctl show " + unit + " --property=ActiveState --value) = activating")
    machine.crash()
    machine.start()
    machine.wait_for_unit("korri-plugin-host.service")
    machine.wait_for_unit(unit)
    machine.wait_for_unit(clock["unit"])
    assert_ports(update, True)
    assert_ports(clock, True)
    machine.succeed("test -e " + report["state_directory"] + "/retained-data")
    machine.wait_until_succeeds(ts + " ping --timeout=5s --c=1 cache")
    machine.wait_until_succeeds("ping -c 1 -W 2 " + peer_ip)
    restored = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    assert restored["package"] == update["package"]
    assert restored["provenance"] == update["provenance"]
    assert restored == before_failed_update
    assert machine.succeed("readlink " + lifecycle_root + "/previous").strip() == restored["previous"]["package"]
    machine.succeed("test ! -L /nix/var/nix/gcroots/korri-plugin-host/" + unit.removesuffix(".service") + "/pending")
    pid = machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip()
    machine.succeed("iptables -w -F korri-plugins; ip6tables -w -F korri-plugins")
    machine.succeed("korri-plugin restore-all")
    machine.succeed("korri-plugin restore-all")
    for tool in ["iptables", "ip6tables"]:
        assert machine.succeed(tool + " -w -S INPUT").splitlines()[1] == "-A INPUT -j korri-plugins"
    assert_ports(update, True)
    assert_ports(clock, True)
    assert machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip() == pid
    machine.succeed("korri-plugin disable @korri:tailscale")
    assert_ports(update, False)
    assert_ports(clock, True)
    machine.fail("systemctl is-active " + unit)
    machine.fail("ip link show tailscale0")
    assert json.loads(machine.succeed("ip -j rule show")) == ipv4_rules
    assert json.loads(machine.succeed("ip -6 -j rule show")) == ipv6_rules
    report = repository_inspect(source_a, "v1")
    machine.succeed("korri-plugin repository update @korri:tailscale v1 " + report["approval"])
    machine.fail("systemctl is-active " + unit)
    machine.succeed("korri-plugin repository remove " + source_a)
    cache.succeed("systemctl stop nginx.service")
    cache.succeed("systemctl stop nix-serve.service")
    # The previous receipt, not the unavailable catalog/cache, owns rollback.
    offline_current = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    machine.succeed("korri-plugin restore @korri:tailscale")
    offline_previous = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    for key in ["package", "provenance", "approval"]:
        assert offline_previous[key] == offline_current["previous"][key]
        assert offline_previous["previous"][key] == offline_current[key]
    assert offline_previous["desired"] == {"state": "Disabled"}
    machine.fail("systemctl is-active " + unit)
    machine.succeed("korri-plugin restore @korri:tailscale")
    assert json.loads(machine.succeed("korri-plugin status @korri:tailscale")) == offline_current
    machine.succeed("test $(find " + lifecycle_root + " -type l | wc -l) = 2")
    cache.succeed("systemctl start nix-serve.service")
    assert json.loads(machine.succeed("korri-plugin status @korri:tailscale"))["provenance"]["source_url"] == source_a
    machine.succeed("korri-plugin enable @korri:tailscale")
    machine.succeed("korri-plugin restore-all")
    machine.succeed("korri-plugin disable @korri:tailscale")
    # Recovery owns only its bounded private staging entries, even offline.
    machine.succeed("mkdir -m 700 /var/lib/korri-plugin-host/staging/download-12345678")
    machine.succeed("touch /var/lib/korri-plugin-host/staging/download-12345678/partial")
    machine.succeed("korri-plugin restore-all")
    machine.fail("test -e /var/lib/korri-plugin-host/staging/download-12345678")
    machine.succeed("korri-plugin remove @korri:tailscale")
    machine.fail("korri-plugin status @korri:tailscale")
    machine.succeed("test -e " + report["state_directory"] + "/retained-data")
    report = install("${tailscalePackage}")
    machine.succeed("korri-plugin remove @korri:tailscale --purge")
    machine.fail("test -e " + report["state_directory"] + "/retained-data")
    # Service removal on update keeps per-ID data. Purge must still clean it
    # after reboot, when neither the selected package nor /run has that unit.
    machine.succeed("touch " + clock["state_directory"] + "/retained-data")
    empty_clock = inspect("${emptyClock}")
    machine.succeed("korri-plugin update @example:clock http://cache:5000 ${emptyClock} " + empty_clock["approval"])
    machine.fail("test -e /run/systemd/system/" + clock["unit"])
    machine.succeed("test -e " + clock["state_directory"] + "/retained-data")
    assert_ports(clock, False)
    machine.crash()
    machine.start()
    machine.wait_for_unit("korri-plugin-host.service")
    machine.succeed("test -e " + clock["state_directory"] + "/retained-data")
    machine.fail("test -e /run/systemd/system/" + clock["unit"])
    machine.succeed("korri-plugin remove @example:clock --purge")
    machine.fail("test -e " + clock["state_directory"])
    machine.fail("test -L " + clock["state_directory"])
    machine.fail("test -e /var/lib/private/" + clock["unit"].removesuffix(".service"))
    machine.fail("korri-plugin status @example:clock")
    machine.fail("test -e /run/systemd/system/" + clock["unit"])
    machine.fail("test -e /run/systemd/system/" + clock["unit"] + ".d")
    assert_ports(clock, False)
    assert_ports(report, False)

    # Kill-equivalent boundary: unit file is durable, but daemon-reload/start
    # never ran. Restore must not require an already loaded systemd unit.
    staged = install("${tailscalePackage}")
    machine.succeed("printf %s " + shlex.quote(staged["unit_configuration"]) + " > /run/systemd/system/" + staged["unit"])
    machine.succeed("ln -s ${tailscalePackage} /nix/var/nix/gcroots/korri-plugin-host/" + staged["unit"].removesuffix(".service") + "/pending")
    machine.succeed("korri-plugin restore-all")
    machine.fail("systemctl is-active " + staged["unit"])
    machine.succeed("korri-plugin remove @korri:tailscale --purge")
    # Revocation changes START authority, not the exact receipt's authority to
    # stop and run its already approved native cleanup. Exercise both forms.
    bindings_path = "/etc/korri-plugin-host/publishers.json"
    bindings = json.loads(machine.succeed("cat " + bindings_path))
    machine.succeed("mv " + bindings_path + " " + bindings_path + ".saved")

    def write_bindings(value):
        machine.succeed("printf %s " + shlex.quote(json.dumps(value)) + " > " + bindings_path)

    for revocation in ["removed", "rotated"]:
        write_bindings(bindings)
        revoked = install("${tailscalePackage}")
        revoked_unit = revoked["unit"]
        revoked_root = "/nix/var/nix/gcroots/korri-plugin-host/" + revoked_unit.removesuffix(".service")
        revoked_update = inspect("${updated}")
        machine.succeed("korri-plugin update @korri:tailscale http://cache:5000 ${updated} " + revoked_update["approval"])
        machine.succeed("korri-plugin enable @korri:tailscale")
        machine.succeed("touch " + revoked["state_directory"] + "/revocation-data")
        changed_bindings = json.loads(json.dumps(bindings))
        if revocation == "removed":
            del changed_bindings["@korri"]
        else:
            changed_bindings["@korri"]["publicKey"] = upstream_key
        write_bindings(changed_bindings)
        before_revoked_swap = machine.succeed("korri-plugin status @korri:tailscale")
        machine.fail("korri-plugin restore @korri:tailscale")
        assert machine.succeed("korri-plugin status @korri:tailscale") == before_revoked_swap
        machine.succeed("test -L " + revoked_root + "/previous")
        machine.fail("korri-plugin enabled-packages")
        machine.fail("korri-plugin enable @korri:tailscale")
        # Deactivation must not recover an Enabled receipt first, even when an
        # interrupted operation left a GC root. It still requires exact approval.
        receipt_path = "/var/lib/korri-plugin-host/" + revoked_unit.removesuffix(".service") + "/selection.json"
        receipt = json.loads(machine.succeed("cat " + receipt_path))

        def write_receipt(value):
            machine.succeed("printf %s " + shlex.quote(json.dumps(value)) + " > " + receipt_path + "; sync")

        write_receipt(dict(receipt, approval="0" * 64))
        for command in ["disable", "remove"]:
            error = machine.fail("korri-plugin " + command + " @korri:tailscale 2>&1")
            assert "no longer matches its approval" in error, error
        machine.succeed("systemctl is-active " + revoked_unit)
        write_receipt(receipt)
        machine.succeed("ln -s " + revoked["package"] + " " + revoked_root + "/pending")
        machine.succeed("korri-plugin disable @korri:tailscale")
        machine.fail("test -L " + revoked_root + "/pending")
        machine.fail("systemctl is-active " + revoked_unit)
        machine.fail("ip link show tailscale0")
        assert json.loads(machine.succeed("ip -j rule show")) == ipv4_rules
        assert json.loads(machine.succeed("ip -6 -j rule show")) == ipv6_rules
        machine.succeed("test -e " + revoked["state_directory"] + "/revocation-data")
        assert json.loads(machine.succeed("korri-plugin status @korri:tailscale"))["desired"] == {"state": "Disabled"}
        machine.fail("korri-plugin enable @korri:tailscale")
        machine.succeed("korri-plugin restore-all")
        machine.fail("systemctl is-active " + revoked_unit)
        write_bindings(bindings)
        machine.succeed("korri-plugin enable @korri:tailscale")
        write_bindings(changed_bindings)
        # Ordinary recovery must stop a revoked running unit, not take the
        # healthy-unit shortcut. Pending recovery must also refuse a new start.
        machine.fail("korri-plugin restore-all")
        machine.fail("systemctl is-active " + revoked_unit)
        machine.succeed("ln -s " + revoked["package"] + " " + revoked_root + "/pending")
        machine.fail("korri-plugin restore-all")
        machine.fail("systemctl is-active " + revoked_unit)
        machine.succeed("test -L " + revoked_root + "/pending")
        # Kill-equivalent boundary: disable intent reached disk but cleanup did
        # not run. Recovery uses that intent with no current signing authority.
        write_receipt(dict(receipt, desired={"state": "Disabled"}))
        machine.succeed("korri-plugin restore-all")
        machine.fail("test -L " + revoked_root + "/pending")
        machine.fail("systemctl is-active " + revoked_unit)
        write_bindings(bindings)
        machine.succeed("korri-plugin enable @korri:tailscale")
        write_bindings(changed_bindings)
        machine.succeed("ln -s " + revoked["package"] + " " + revoked_root + "/pending")
        machine.succeed("korri-plugin remove @korri:tailscale --purge")
        machine.fail("systemctl is-active " + revoked_unit)
        machine.fail("ip link show tailscale0")
        machine.fail("korri-plugin status @korri:tailscale")
        machine.fail("test -e " + revoked["state_directory"] + "/revocation-data")
        machine.fail("test -L " + revoked_root + "/active")
        machine.fail("test -L " + revoked_root + "/previous")
        machine.fail("korri-plugin enable @korri:tailscale")
    machine.succeed("rm " + bindings_path + "; mv " + bindings_path + ".saved " + bindings_path)

    failed_cleanup = install("${unclean}")
    machine.succeed("korri-plugin enable @example:unclean")
    machine.succeed("mv " + bindings_path + " " + bindings_path + ".saved")
    write_bindings(dict(bindings, **{"@example": dict(bindings["@example"], publicKey=upstream_key)}))
    machine.fail("korri-plugin disable @example:unclean")
    assert_ports(failed_cleanup, False)
    assert json.loads(machine.succeed("korri-plugin status @example:unclean"))["desired"] == {"state": "Disabled"}
    machine.fail("systemctl is-active " + failed_cleanup["unit"])
    machine.fail("korri-plugin restore-all")
    machine.fail("systemctl is-active " + failed_cleanup["unit"])
    machine.fail("korri-plugin enable @example:unclean")
    machine.fail("korri-plugin remove @example:unclean --purge")
    assert json.loads(machine.succeed("korri-plugin status @example:unclean"))["desired"] == {"state": "Removed", "purge": True}
    machine.fail("korri-plugin restore-all")
    machine.fail("systemctl is-active " + failed_cleanup["unit"])
    machine.succeed("rm " + bindings_path + "; mv " + bindings_path + ".saved " + bindings_path)
    machine.succeed("korri-plugin status @example:unclean")
    machine.succeed("test -L /nix/var/nix/gcroots/korri-plugin-host/" + failed_cleanup["unit"].removesuffix(".service") + "/active")
    assert machine.succeed("readlink -f /run/current-system").strip() == generation
  '';
}
