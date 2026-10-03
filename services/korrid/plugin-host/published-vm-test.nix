# Acceptance consumes immutable outputs already substituted by the caller.
# No plugin producers, test signing keys, NAR cache, or plugin derivations.
# publishedPaths has the selected SSH, mGBA, FAKE-08 and Starter pack paths;
# metadata is the checked union of the corresponding offline proof archives.
{
  pkgs,
  hostModule,
  hostPackage,
  korridPackage,
  publishedPaths,
  metadata,
}:
let
  lib = pkgs.lib;
  binding =
    (import ../../../nix/product/requirements.nix { korri = { }; }).constants.publishers."@korri";
  paths = [
    publishedPaths.ssh
    publishedPaths.mgba
    publishedPaths.fake08
    publishedPaths.starterPack
  ];
  # Reject recipe-backed values even if their output exists in a warm store.
  exact =
    path:
    builtins.isString path
    && lib.hasPrefix "/nix/store/" path
    && builtins.all (name: !(lib.hasSuffix ".drv" name)) (
      builtins.attrNames (builtins.getContext path)
    );
  closure = pkgs.closureInfo { rootPaths = paths; };
  image = import ./image-seed.nix {
    inherit pkgs hostPackage;
    publishers."@korri" = binding;
    pluginPackages = paths;
  };
  # Run the existing image producer unchanged, including its host-generated
  # approval and unit-name policy. No fixture builder or parallel receipt format.
  imageSeed = pkgs.writeShellApplication {
    name = "published-image-seed";
    runtimeInputs = [ pkgs.coreutils ];
    text = ''
      mkdir -p ./files
      ${image.populateRootCommands}
    '';
  };
  storage = pkgs.runCommand "published-plugin-route-storage" { } ''
    mkdir -p "$out/catalog" "$out/roms"
    cp ${../../../docs/research/retroarch-plugin-route/device.yaml} "$out/device.yaml"
    cp ${../../../docs/research/retroarch-plugin-route/catalog/games.yaml} "$out/catalog/games.yaml"
    cp ${../../../docs/research/retroarch-plugin-route/catalog/releases.yaml} "$out/catalog/releases.yaml"
    # Only route resolution uses this marker. It is not executable game content.
    printf route-only-marker > "$out/roms/wl4.gba"
  '';
  deviceConfig = pkgs.writeText "published-plugin-vm.toml" ''
    label = "published-plugin-vm"
  '';
in
assert pkgs.stdenv.hostPlatform.system == "x86_64-linux";
assert builtins.all exact paths;
pkgs.testers.runNixOSTest {
  name = "korri-published-plugin-acceptance";
  includeTestScriptReferences = false;
  nodes = {
    peer = {
      environment.systemPackages = [ pkgs.openssh ];
    };
    machine =
      { lib, ... }:
      {
        imports = [
          hostModule
          (import ../nixos-module.nix {
            korri.packages.${pkgs.stdenv.hostPlatform.system}.korrid = korridPackage;
          })
        ];
        services.korri.pluginHost = {
          enable = true;
          package = hostPackage;
          publishers."@korri" = binding;
        };
        # Keep the genuine production namespace/key/URL binding. Never redirect
        # it to a fixture cache. Only the local archive registers proofs.
        nix.settings.substituters = lib.mkForce [ ];
        systemd.services.korri-plugin-offline-proofs =
          import ../../../nix/product/published-plugin-offline-unit.nix
            {
              inherit pkgs metadata;
              pluginPackages = paths;
              publicKey = binding.publicKey;
            };
        # Install egress refusal at every boot, before either proof import or
        # receipt recovery. A post-boot test command cannot prove this order.
        systemd.services.published-offline = {
          requiredBy = [
            "korri-plugin-offline-proofs.service"
            "korri-plugin-host.service"
          ];
          before = [
            "korri-plugin-offline-proofs.service"
            "korri-plugin-host.service"
          ];
          after = [ "firewall.service" ];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };
          path = [ pkgs.iptables ];
          script = ''
            iptables -w -N published-offline
            iptables -w -A published-offline -o lo -j RETURN
            iptables -w -A published-offline -d 192.168.1.0/24 -j RETURN
            iptables -w -A published-offline -j REJECT
            iptables -w -I OUTPUT 1 -j published-offline
            ip6tables -w -N published-offline
            ip6tables -w -A published-offline -o lo -j RETURN
            ip6tables -w -A published-offline -j REJECT
            ip6tables -w -I OUTPUT 1 -j published-offline
          '';
        };
        users.users.plugin-user = {
          isNormalUser = true;
          uid = 1000;
          group = "users";
          hashedPassword = "!";
        };
        services.korridLinuxDevice = {
          enable = true;
          package = korridPackage;
          uid = 976;
          gid = 976;
          runtimeUser = "plugin-user";
          runtimeUid = 1000;
          runtimeGid = 100;
          inputdUid = 977;
          controlGid = 977;
          localSignerUid = 978;
          localSignerGid = 978;
          inherit deviceConfig;
          storageRoot = "/var/lib/published-plugin-storage";
          streamPrivateStateRoot = "/var/lib/korri-sunshine-private";
          # The production module requires a relay. This is the existing
          # loopback-only VM contract; no external federation participates.
          relays = [ "ws://127.0.0.1:49000" ];
        };
        systemd.tmpfiles.rules = [ "d /var/lib/korri-sunshine-private 0700 root root -" ];
        systemd.services.korrid.environment = {
          KORRID_MODE = lib.mkForce "brain";
          KORRID_ADDRESS = lib.mkForce "127.0.0.1:49117";
          KORRID_RPC_CAPABILITY = "published-plugin-vm-capability";
          KORRI_LOCAL_STORAGE_ROOT = "/var/lib/published-plugin-storage";
        };
        system.activationScripts.publishedPluginStorage.text = ''
          install -d -m 0755 /var/lib/published-plugin-storage
          cp -R --no-preserve=mode,ownership ${storage}/. /var/lib/published-plugin-storage/
        '';
        environment.systemPackages = [
          pkgs.curl
          pkgs.python3
          pkgs.openssh
          korridPackage
          imageSeed
        ];
        # These are prebuilt path references, not plugin .drv dependencies.
        virtualisation.additionalPaths = paths ++ [
          metadata
          closure
        ];
        virtualisation.memorySize = 3072;
        virtualisation.diskSize = 12288;
      };
  };
  testScript = ''
    import json
    import shlex

    start_all()
    machine.wait_for_unit("korri-plugin-host.service")
    machine.wait_for_unit("korrid.service")
    peer.wait_for_unit("multi-user.target")
    paths = ${builtins.toJSON (map builtins.unsafeDiscardStringContext paths)}
    ssh_path, mgba_path, fake08_path, pack_path = paths
    expected_packages = {"@korri:ssh": ssh_path, "@korri:mgba": mgba_path, "@korri:fake08": fake08_path, "@korri:starter-pack": pack_path}
    cache_url = ${builtins.toJSON binding.cacheUrl}
    package_args = " ".join(shlex.quote(path) for path in paths)
    generation = machine.succeed("readlink -f /run/current-system").strip()
    nix = 'nix --extra-experimental-features nix-command --option substituters "" '

    def offline():
        # Block external traffic, including the bound publisher URL. Preserve
        # only loopback and the test VLAN used by the actual SSH client.
        machine.wait_for_unit("published-offline.service")
        machine.succeed("iptables -w -C OUTPUT -j published-offline; ip6tables -w -C OUTPUT -j published-offline")
        machine.fail("curl --connect-timeout 2 --max-time 4 --fail " + shlex.quote(cache_url + "nix-cache-info"))
        assert machine.succeed("nix config show substituters").strip() == ""
        assert machine.succeed("nix config show max-jobs").strip() == "0"
        assert machine.succeed("nix config show builders").strip() == ""

    offline()
    closure_paths = machine.succeed("cat ${closure}/store-paths").splitlines()
    assert all(not path.endswith(".drv") for path in closure_paths)

    # An independent local database starts with zero signatures, even if VM
    # system paths overlap the payload closure. closureInfo registration carries
    # only hashes/references; no source-store signatures or ultimate bit. Content
    # verification reads the actual canonical store bytes, not another copy.
    isolated = nix + "--store 'local?state=/var/lib/unsigned' "
    machine.succeed("nix-store --store 'local?state=/var/lib/unsigned' --load-db < ${closure}/registration")
    unsigned_info = json.loads(machine.succeed(isolated + "path-info --json --recursive " + package_args))
    entries = unsigned_info.values() if isinstance(unsigned_info, dict) else unsigned_info
    assert all(not info.get("signatures") and not info.get("ultimate", False) for info in entries)
    mgba_info = json.loads(machine.succeed(nix + "path-info --json " + shlex.quote(mgba_path)))
    info = mgba_info[mgba_path] if isinstance(mgba_info, dict) else mgba_info[0]
    dependency = next(path for path in info["references"] if path not in paths)
    machine.succeed("cp -a ${metadata} /run/incomplete-proofs; chmod -R u+w /run/incomplete-proofs")
    proof = dependency.split("/")[-1].split("-")[0] + ".narinfo"
    machine.succeed("test -f /run/incomplete-proofs/" + proof + "; rm /run/incomplete-proofs/" + proof)
    # Nix versions differ on whether copy-sigs itself fails on a missing proof;
    # the required terminal gate is recursive signature AND content verification.
    machine.execute(isolated + "store copy-sigs --recursive --substituter file:///run/incomplete-proofs " + package_args)
    failure = machine.fail(isolated + "store verify --recursive --sigs-needed 1 " + package_args + " 2>&1", timeout=300)
    assert dependency in failure, failure
    assert "untrusted" in failure.lower() or "signature" in failure.lower(), failure
    machine.fail("korri-plugin status @korri:mgba")
    assert json.loads(machine.succeed("korri-plugin enabled-packages")) == []

    # Repair only the omitted proof, not bytes or publisher bindings, and
    # verify this exact formerly unsigned closure with actual content hashing.
    machine.succeed(isolated + "store copy-sigs --recursive --substituter file://${metadata} " + package_args, timeout=300)
    machine.succeed(isolated + "store verify --recursive --sigs-needed 1 " + package_args, timeout=300)
    # The real host uses the VM's canonical store. Register the same original
    # offline proofs there and require full closure/byte verification again.
    machine.succeed(nix + "store copy-sigs --recursive --substituter file://${metadata} " + package_args, timeout=300)
    machine.succeed(nix + "store verify --recursive --sigs-needed 1 " + package_args, timeout=300)

    # Raw-cache inspect/install initializes the bound cache even for local
    # roots. It is unavailable offline. This tests the actual image installation
    # path instead: existing seed producer -> approved receipts/roots -> restore.
    machine.succeed("systemctl stop korrid.service korri-plugin-host.service; mkdir -p /run/published-image-stage; cd /run/published-image-stage; published-image-seed")
    machine.succeed("cp -a /run/published-image-stage/files/var/lib/korri-plugin-host/. /var/lib/korri-plugin-host/; cp -a /run/published-image-stage/files/nix/var/nix/gcroots/korri-plugin-host/. /nix/var/nix/gcroots/korri-plugin-host/")
    # First boot applies the real host tmpfiles policy before recovery. Copying
    # the image staging parent directories alone does not reproduce that step.
    machine.succeed("systemd-tmpfiles --create --prefix=/var/lib/korri-plugin-host --prefix=/nix/var/nix/gcroots/korri-plugin-host")
    machine.succeed("rm -f /run/korri-plugin-host/enabled-packages.json")

    def seeded(id, package):
        name = machine.succeed("korri-plugin unit-name " + shlex.quote(id)).strip()
        receipt_path = "/var/lib/korri-plugin-host/" + name + "/selection.json"
        receipt = json.loads(machine.succeed("cat " + receipt_path))
        assert receipt["id"] == id and receipt["package"] == package, receipt
        assert receipt["desired"] == {"state": "Enabled"} and receipt["previous"] is None, receipt
        assert receipt["provenance"] == {"kind": "RawCache", "cache_url": cache_url}, receipt
        assert machine.succeed("stat -c '%U:%a' " + receipt_path).strip() == "root:600"
        assert machine.succeed("stat -c '%U:%a' /var/lib/korri-plugin-host/" + name).strip() == "root:700"
        root = "/nix/var/nix/gcroots/korri-plugin-host/" + name
        assert machine.succeed("stat -c '%U:%a' " + root).strip() == "root:700"
        assert machine.succeed("readlink " + root + "/active").strip() == package
        return receipt_path, receipt

    ssh_receipt_path, ssh_receipt = seeded("@korri:ssh", ssh_path)
    mgba_receipt_path, mgba_receipt = seeded("@korri:mgba", mgba_path)
    fake08_receipt_path, fake08_receipt = seeded("@korri:fake08", fake08_path)
    pack_receipt_path, pack_receipt = seeded("@korri:starter-pack", pack_path)
    pack_manifest = json.loads(machine.succeed("cat " + pack_path + "/manifest.json"))
    assert pack_manifest["requires"] == [fake08_path], pack_manifest
    ssh_manifest = json.loads(machine.succeed("cat " + ssh_path + "/manifest.json"))
    assert len(ssh_manifest["services"]) == 1, ssh_manifest
    native_name, native_source = next(iter(ssh_manifest["services"].items()))
    ssh_unit = native_name + ".service"

    def ports(enabled):
        for tool in ["iptables", "ip6tables"]:
            rules = machine.succeed(tool + " -w -S").splitlines()
            owned = [line for line in rules if line.startswith("-A korri-plugins ") and "--dport 2222 " in line]
            assert len(owned) == (1 if enabled else 0), rules

    machine.fail("systemctl cat sshd.service")
    machine.fail("test -e /etc/ssh/sshd_config")
    machine.succeed("getent passwd sshd; test -f /etc/pam.d/sshd")
    # Tamper ONLY the seeded approval, never the manifest, package or binding.
    # Restore must refuse before staging/starting the real SSH daemon or ports.
    corrupt = dict(ssh_receipt, approval="0" * 64)
    machine.succeed("printf %s " + shlex.quote(json.dumps(corrupt)) + " > " + ssh_receipt_path)
    refusal = machine.fail("korri-plugin restore-all 2>&1", timeout=300)
    assert "no longer matches its approval" in refusal, refusal
    ports(False)
    machine.fail("systemctl is-active " + ssh_unit)
    machine.fail("test -e /run/systemd/system/" + ssh_unit)
    assert machine.succeed("ss -ltnH 'sport = :2222'").strip() == ""
    remaining_packages = {id: package for id, package in expected_packages.items() if id != "@korri:ssh"}
    assert {p["id"]: p["package"] for p in json.loads(machine.succeed("korri-plugin enabled-packages"))} == remaining_packages
    assert {p["id"]: p["package"] for p in json.loads(machine.succeed("cat /run/korri-plugin-host/enabled-packages.json"))} == remaining_packages
    # Repair from the original approved image producer bytes, not a migration
    # or a hand-authored receipt. The current host re-derives approval offline.
    original = "/run/published-image-stage/files" + ssh_receipt_path
    machine.succeed("cp -a " + original + " " + ssh_receipt_path + "; korri-plugin restore-all", timeout=300)
    machine.succeed("cmp " + original + " " + ssh_receipt_path)
    machine.succeed("systemctl start korri-plugin-host.service korrid.service")
    reports = {report["id"]: report for report in json.loads(machine.succeed("korri-plugin enabled-packages"))}
    assert set(reports) == set(expected_packages), reports
    ssh = reports["@korri:ssh"]
    mgba = reports["@korri:mgba"]
    for id, package in expected_packages.items():
        report = reports[id]
        assert report["package"] == package, report
        manifest = json.loads(machine.succeed("cat " + package + "/manifest.json"))
        for field in ["entry", "sources", "files"]:
            assert report[field] == manifest[field], (field, report, manifest)
    assert ssh["approval"] == ssh_receipt["approval"]
    assert mgba["approval"] == mgba_receipt["approval"]
    assert reports["@korri:fake08"]["approval"] == fake08_receipt["approval"]
    assert reports["@korri:starter-pack"]["approval"] == pack_receipt["approval"]
    assert reports["@korri:starter-pack"]["native_units"] == {}
    # The signed pack must recover through the same exact graph approval as
    # ordinary installation. A changed seeded digest cannot authorize it.
    machine.succeed("systemctl stop korrid.service korri-plugin-host.service")
    corrupt_pack = dict(pack_receipt, approval="0" * 64)
    machine.succeed("printf %s " + shlex.quote(json.dumps(corrupt_pack)) + " > " + pack_receipt_path)
    refusal = machine.fail("korri-plugin restore-all 2>&1", timeout=300)
    assert "no longer matches its approval" in refusal, refusal
    remaining_packages = {id: package for id, package in expected_packages.items() if id != "@korri:starter-pack"}
    assert {p["id"]: p["package"] for p in json.loads(machine.succeed("korri-plugin enabled-packages"))} == remaining_packages
    assert {p["id"]: p["package"] for p in json.loads(machine.succeed("cat /run/korri-plugin-host/enabled-packages.json"))} == remaining_packages
    original_pack = "/run/published-image-stage/files" + pack_receipt_path
    machine.succeed("cp -a " + original_pack + " " + pack_receipt_path + "; korri-plugin restore-all", timeout=300)
    machine.succeed("cmp " + original_pack + " " + pack_receipt_path)
    machine.succeed("systemctl start korri-plugin-host.service korrid.service")
    refusal = machine.fail("korri-plugin disable @korri:fake08 2>&1", timeout=300)
    assert "actively required by @korri:starter-pack" in refusal, refusal
    machine.succeed("korri-plugin disable @korri:starter-pack; korri-plugin disable @korri:fake08")
    machine.succeed("korri-plugin enable @korri:starter-pack; korri-plugin restore-all", timeout=300)
    assert {p["id"]: p["package"] for p in json.loads(machine.succeed("korri-plugin enabled-packages"))} == expected_packages
    assert ssh["policy"].startswith("policy-root-v3:")
    assert "DEVICE-WIDE ROOT AUTHORITY" in ssh["warning"]
    assert len(ssh["native_units"]) == 1
    assert next(iter(ssh["native_units"].values()))["user"] == "root"
    assert "DynamicUser=no" in ssh["unit_configuration"]
    assert ssh["ports"] == {"allowedTCPPorts": [2222], "allowedUDPPorts": []}
    assert ssh["unit"] == ssh_unit
    assert mgba["native_units"] == {}
    machine.wait_for_unit(ssh_unit)
    assert machine.succeed("systemctl show " + ssh_unit + " --property=User --value").strip() == "root"
    machine.succeed("cmp " + shlex.quote(native_source) + " /run/systemd/system/" + ssh_unit)
    machine.wait_for_open_port(2222)
    ports(True)
    peer.succeed('ssh-keygen -q -t ed25519 -N "" -f /root/login; ssh-keygen -q -t ed25519 -N "" -f /root/rejected')
    key = peer.succeed("cat /root/login.pub").strip()
    machine.succeed("mkdir -p /etc/ssh/authorized_keys.d; printf '%s\\n' " + shlex.quote(key) + " > /etc/ssh/authorized_keys.d/root")
    host_key = ssh["state_directory"] + "/ssh_host_ed25519_key"
    fingerprint = machine.succeed("sha256sum " + host_key).strip()
    assert machine.succeed("stat -c '%U:%a' " + host_key).strip() == "root:600"
    public = machine.succeed("cat " + host_key + ".pub").strip()
    peer.succeed("printf '%s\\n' " + shlex.quote("[machine]:2222 " + public) + " > /root/known-hosts")
    client = "timeout -k 5s 20s ssh -n -F /dev/null -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -o ConnectTimeout=2 -o UserKnownHostsFile=/root/known-hosts -p 2222 "
    def login(key="login", success=True):
        operation = peer.succeed if success else peer.fail
        return operation(client + "-i /root/" + key + " root@machine id -u 2>&1", timeout=30).strip()
    assert login() == "0"
    assert "Permission denied (publickey)" in login("rejected", False)
    peer.fail(client + "-o PubkeyAuthentication=no root@machine true", timeout=30)
    machine.succeed("korri-plugin disable @korri:ssh; korri-plugin restore-all")
    ports(False)
    machine.fail("systemctl is-active " + ssh["unit"])
    login(success=False)
    machine.succeed("korri-plugin enable @korri:ssh")
    assert machine.succeed("sha256sum " + host_key).strip() == fingerprint

    machine.succeed("korri-plugin restore-all")
    enabled = json.loads(machine.succeed("cat /run/korri-plugin-host/enabled-packages.json"))
    assert {p["id"]: p["package"] for p in enabled} == expected_packages, enabled
    def games():
        request = json.dumps({"_tag": "app.local-games.list", "payload": {}})
        result = json.loads(machine.succeed("curl --fail --silent http://127.0.0.1:49117/rpc -H 'content-type: application/json' -H 'authorization: Bearer published-plugin-vm-capability' -d " + shlex.quote(request)))
        assert result["outcome"]["_tag"] == "Ok", result
        return result["outcome"]["payload"]["games"]
    machine.wait_for_open_port(49117)
    assert [game["id"] for game in games()] == ["01K4J6K8Y00000000000000002"]
    machine.shutdown()
    machine.start()
    machine.wait_for_unit("korri-plugin-host.service")
    machine.wait_for_unit("korrid.service")
    offline()
    machine.wait_for_open_port(2222)
    ports(True)
    assert machine.succeed("sha256sum " + host_key).strip() == fingerprint
    assert login() == "0"
    machine.succeed("korri-plugin restore-all")
    assert json.loads(machine.succeed("cat /run/korri-plugin-host/enabled-packages.json")) == enabled
    assert [game["id"] for game in games()] == ["01K4J6K8Y00000000000000002"]
    machine.succeed("korri-plugin disable @korri:starter-pack; korri-plugin disable @korri:fake08; korri-plugin disable @korri:ssh; korri-plugin disable @korri:mgba; korri-plugin restore-all")
    ports(False)
    login(success=False)
    assert games() == []
    assert json.loads(machine.succeed("korri-plugin enabled-packages")) == []
    assert machine.succeed("readlink -f /run/current-system").strip() == generation
    machine.succeed(nix + "store verify --recursive --sigs-needed 1 " + package_args, timeout=300)
  '';
}
