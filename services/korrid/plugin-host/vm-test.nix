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
  alternate = pkgs.runCommand "independent-plugin" { } ''
    mkdir -p "$out/bin"
    ln -s ${pkgs.coreutils}/bin/sleep "$out/bin/sleep"
    cat > "$out/plugin.ts" <<'EOF'
    ({namespace:'@example',name:'clock',title:'Clock',contributes:{daemons:[{Type:'exec',ExecStart:['bin/sleep','3600'],CapabilityBoundingSet:[]}]}})
    EOF
  '';
  updated = pkgs.runCommand "tailscale-plugin-update" { } ''
    mkdir -p "$out/bin"
    ln -s ${pkgs.tailscale}/bin/tailscaled "$out/bin/tailscaled"
    ln -s ${pkgs.tailscale}/bin/tailscale "$out/bin/tailscale"
    sed 's/title: "Tailscale"/title: "Tailscale updated"/' ${../../../plugins/tailscale/plugin.ts} > "$out/plugin.ts"
  '';
  broken = pkgs.runCommand "tailscale-plugin-failed-update" { } ''
    mkdir -p "$out/bin"
    ln -s ${pkgs.coreutils}/bin/false "$out/bin/daemon"
    cat > "$out/plugin.ts" <<'EOF'
    ({namespace:'@korri',name:'tailscale',title:'Unhealthy candidate',contributes:{daemons:[{Type:'exec',ExecStart:['bin/daemon'],CapabilityBoundingSet:[]}]}})
    EOF
  '';
  interrupted = pkgs.runCommand "tailscale-plugin-interrupted-update" { } ''
    mkdir -p "$out/bin"
    ln -s ${pkgs.coreutils}/bin/sleep "$out/bin/sleep"
    cat > "$out/plugin.ts" <<'EOF'
    ({namespace:'@korri',name:'tailscale',title:'Pending candidate',contributes:{daemons:[{Type:'notify',ExecStart:['bin/sleep','30'],CapabilityBoundingSet:[]}]}})
    EOF
  '';
  unclean = pkgs.runCommand "plugin-cleanup-failure" { } ''
    mkdir -p "$out/bin"
    ln -s ${pkgs.coreutils}/bin/sleep "$out/bin/sleep"
    ln -s ${pkgs.coreutils}/bin/false "$out/bin/false"
    cat > "$out/plugin.ts" <<'EOF'
    ({namespace:'@example',name:'unclean',contributes:{daemons:[{Type:'exec',ExecStart:['bin/sleep','3600'],ExecStopPost:['bin/false'],CapabilityBoundingSet:[]}]}})
    EOF
  '';
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
        ];
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
      { ... }:
      {
        imports = [ hostModule ];
        services.korri.pluginHost.enable = true;
        services.korri.pluginHost.package = hostPackage;
        services.korri.pluginHost.officialCatalogUrl = "https://cache/repositories/official.json";
        nix.settings.trusted-public-keys = [ publicKey ];
        security.pki.certificateFiles = [ "${certificate}/cert.pem" ];
        environment.systemPackages = [ pkgs.jq ];
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
    machine.succeed("korri-plugin restore")
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
    assert machine.succeed(report["package"] + "/bin/tailscale --socket=" + report["runtime_directory"] + "/tailscaled.sock status --json | ${pkgs.jq}/bin/jq -r .BackendState").strip() == "NeedsLogin"
    machine.succeed("touch " + report["state_directory"] + "/retained-data")

    # A disposable local tailnet uses no owner's credentials or public service.
    cache.wait_for_unit("headscale.service")
    cache.succeed("headscale users create plugin-test")
    authkey = cache.succeed("headscale preauthkeys -u 1 create --reusable").strip()
    cache.wait_for_unit("nginx.service")
    cache.succeed("tailscale up --login-server=https://cache --accept-dns=false --auth-key=" + shlex.quote(authkey))
    ts = report["package"] + "/bin/tailscale --socket=" + report["runtime_directory"] + "/tailscaled.sock"
    machine.succeed(ts + " up --login-server=https://cache --accept-dns=false --auth-key=" + shlex.quote(authkey))
    machine.wait_until_succeeds(ts + " ping --timeout=5s --c=1 cache")
    assert json.loads(machine.succeed(ts + " status --json"))["BackendState"] == "Running"
    peer_ip = cache.succeed("tailscale ip -4").strip()
    machine.wait_until_succeeds("ping -c 1 -W 2 " + peer_ip)

    clock = install("${alternate}")
    machine.succeed("korri-plugin enable @example:clock")
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
    ts = update["package"] + "/bin/tailscale --socket=" + update["runtime_directory"] + "/tailscaled.sock"
    machine.wait_until_succeeds(ts + " ping --timeout=5s --c=1 cache")
    assert machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip() == pid
    candidate = repository_inspect(source_b, "broken")
    machine.fail("korri-plugin repository switch @korri:tailscale " + source_b + " broken " + candidate["approval"])
    machine.wait_for_unit(unit)
    restored = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    assert restored["package"] == update["package"]
    assert restored["provenance"] == update["provenance"]

    pending = repository_inspect(source_b, "pending")
    machine.succeed("systemd-run --unit=interrupted-plugin-update /run/current-system/sw/bin/korri-plugin repository switch @korri:tailscale " + source_b + " pending " + pending["approval"])
    machine.wait_until_succeeds("grep -F " + pending["package"] + " /run/systemd/system/" + unit)
    machine.wait_until_succeeds("test $(systemctl show " + unit + " --property=ActiveState --value) = activating")
    machine.crash()
    machine.start()
    machine.wait_for_unit("korri-plugin-host.service")
    machine.wait_for_unit(unit)
    machine.wait_for_unit(clock["unit"])
    machine.succeed("test -e " + report["state_directory"] + "/retained-data")
    machine.wait_until_succeeds(ts + " ping --timeout=5s --c=1 cache")
    machine.wait_until_succeeds("ping -c 1 -W 2 " + peer_ip)
    restored = json.loads(machine.succeed("korri-plugin status @korri:tailscale"))
    assert restored["package"] == update["package"]
    assert restored["provenance"] == update["provenance"]
    machine.succeed("test ! -L /nix/var/nix/gcroots/korri-plugin-host/" + unit.removesuffix(".service") + "/pending")
    pid = machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip()
    machine.succeed("korri-plugin restore")
    assert machine.succeed("systemctl show " + clock["unit"] + " --property=MainPID --value").strip() == pid
    machine.succeed("korri-plugin disable @korri:tailscale")
    machine.fail("systemctl is-active " + unit)
    machine.fail("ip link show tailscale0")
    assert json.loads(machine.succeed("ip -j rule show")) == ipv4_rules
    assert json.loads(machine.succeed("ip -6 -j rule show")) == ipv6_rules
    report = repository_inspect(source_a, "v1")
    machine.succeed("korri-plugin repository update @korri:tailscale v1 " + report["approval"])
    machine.fail("systemctl is-active " + unit)
    machine.succeed("korri-plugin repository remove " + source_a)
    cache.succeed("systemctl stop nginx.service")
    assert json.loads(machine.succeed("korri-plugin status @korri:tailscale"))["provenance"]["source_url"] == source_a
    machine.succeed("korri-plugin enable @korri:tailscale")
    machine.succeed("korri-plugin restore")
    machine.succeed("korri-plugin disable @korri:tailscale")
    # Recovery owns only its bounded private staging entries, even offline.
    machine.succeed("mkdir -m 700 /var/lib/korri-plugin-host/staging/download-12345678")
    machine.succeed("touch /var/lib/korri-plugin-host/staging/download-12345678/partial")
    machine.succeed("korri-plugin restore")
    machine.fail("test -e /var/lib/korri-plugin-host/staging/download-12345678")
    machine.succeed("korri-plugin remove @korri:tailscale")
    machine.fail("korri-plugin status @korri:tailscale")
    machine.succeed("test -e " + report["state_directory"] + "/retained-data")
    report = install("${tailscalePackage}")
    machine.succeed("korri-plugin remove @korri:tailscale --purge")
    machine.fail("test -e " + report["state_directory"] + "/retained-data")
    machine.succeed("korri-plugin remove @example:clock --purge")

    # Kill-equivalent boundary: unit file is durable, but daemon-reload/start
    # never ran. Restore must not require an already loaded systemd unit.
    staged = install("${tailscalePackage}")
    machine.succeed("printf %s " + shlex.quote(staged["unit_configuration"]) + " > /run/systemd/system/" + staged["unit"])
    machine.succeed("ln -s ${tailscalePackage} /nix/var/nix/gcroots/korri-plugin-host/" + staged["unit"].removesuffix(".service") + "/pending")
    machine.succeed("korri-plugin restore")
    machine.fail("systemctl is-active " + staged["unit"])
    machine.succeed("korri-plugin remove @korri:tailscale --purge")
    failed_cleanup = install("${unclean}")
    machine.succeed("korri-plugin enable @example:unclean")
    machine.fail("korri-plugin remove @example:unclean --purge")
    machine.succeed("korri-plugin status @example:unclean")
    machine.succeed("test -L /nix/var/nix/gcroots/korri-plugin-host/" + failed_cleanup["unit"].removesuffix(".service") + "/active")
    assert machine.succeed("readlink -f /run/current-system").strip() == generation
  '';
}
