# Focused systemd test using the same native BindsTo/After shape as Sunshine.
# The fixture sleeps instead of requiring a GPU, KMS capture or an encoder.
{
  pkgs,
  hostModule,
  hostPackage,
}:
let
  mkPlugin = (import ./interface.nix).mkPlugin { inherit pkgs; };
  host = hostPackage.overrideAttrs { doCheck = false; };
  fixture =
    duration:
    mkPlugin {
      publisher.namespace = "@korri";
      source = pkgs.writeTextDir "plugin.ts" ''
        export const name = "sunshine";
        export const services = ["korri-sunshine"];
      '';
      plugin = _: {
        services.korri-sunshine = {
          unitConfig = {
            After = "korri-compositor.service";
            BindsTo = "korri-compositor.service";
          };
          serviceConfig = {
            Type = "exec";
            ExecStart = "${pkgs.coreutils}/bin/sleep ${duration}";
          };
        };
      };
    };
  plugin = fixture "infinity";
  replacement = fixture "86400";
  # Existing seed and receipt formats, with no live cache or publisher service.
  binding = {
    publicKey = "korri-plugin-test:r7JN4vIsXpQU56Pfw9c9FD4IavDfrsxHH/uXSwxWvSU=";
    cacheUrl = "https://cache.example.invalid";
  };
  # Existing disposable VM cache key. It grants no production authority.
  key = pkgs.writeText "streaming-lifecycle-test-key" "korri-plugin-test:XMn+6POJ5fj568Beg6v8OLo4wMcNKehDPxH+7bUrt0Svsk3i8ixelBTno9/D1z0UPghq8N+uzEcf+5dLDFa9JQ==";
  bindings = pkgs.writeText "streaming-lifecycle-publishers.json" (
    builtins.toJSON {
      "@korri" = binding;
    }
  );
  receiptFor =
    package:
    pkgs.runCommand "streaming-lifecycle-receipt" { } ''
      ${host}/bin/korri-plugin seed-graph ${bindings} ${package} > graph.json
      ${pkgs.jq}/bin/jq -c '.[0]' graph.json > "$out"
    '';
  receipt = receiptFor plugin;
  replacementReceipt = receiptFor replacement;
in
pkgs.testers.runNixOSTest {
  name = "korri-streaming-plugin-lifecycle";
  nodes.machine = {
    imports = [ hostModule ];
    services.korri.pluginHost = {
      enable = true;
      package = host;
      publishers."@korri" = binding;
    };
    systemd.services = {
      seed-streaming-plugin = {
        requiredBy = [ "korri-plugin-host.service" ];
        before = [ "korri-plugin-host.service" ];
        serviceConfig.Type = "oneshot";
        script = ''
          ${pkgs.nix}/bin/nix --extra-experimental-features nix-command store sign --recursive --key-file ${key} ${plugin} ${replacement}
          name="$(${host}/bin/korri-plugin unit-name '@korri:sunshine')"
          install -d -m 0700 "/var/lib/korri-plugin-host/$name" "/nix/var/nix/gcroots/korri-plugin-host/$name"
          if ! test -e "/var/lib/korri-plugin-host/$name/selection.json"; then
            install -m 0600 ${receipt} "/var/lib/korri-plugin-host/$name/selection.json"
            ln -s ${plugin} "/nix/var/nix/gcroots/korri-plugin-host/$name/active"
          fi
        '';
      };
      korrid = {
        wantedBy = [ "multi-user.target" ];
        after = [ "korri-plugin-host.service" ];
        serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
      };
      korri-compositor = {
        wantedBy = [ "multi-user.target" ];
        serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
      };
    };
    # A real generation switch, without changing any plugin selection.
    specialisation.next.configuration.environment.etc."lifecycle-generation".text = "next";
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("korrid.service")
    unit = "korri-sunshine.service"
    relation = "/run/systemd/system/korrid.service.upholds/" + unit
    machine.wait_for_unit(unit)
    assert "korrid.service" in machine.succeed("systemctl show " + unit + " -p UpheldBy --value").split()
    machine.succeed("test -L " + relation)
    # Reproduce the original failure: BindsTo stops the unit, then systemd
    # must start it again without a settings write or restore-all command.
    for parent in ["korrid.service", "korri-compositor.service"]:
        machine.succeed("systemctl restart " + parent)
        machine.wait_for_unit(unit)
    machine.succeed("/run/current-system/specialisation/next/bin/switch-to-configuration test")
    machine.wait_for_unit(unit)
    machine.succeed("grep -Fx next /etc/lifecycle-generation")
    # Replace an image-seeded bundle using the existing seed producer. The
    # ordinary CLI update's cache/signature transport is covered by vm-test.nix;
    # this test stays offline and exercises only the native lifecycle boundary.
    machine.succeed("korri-plugin disable @korri:sunshine")
    name = machine.succeed("korri-plugin unit-name @korri:sunshine").strip()
    machine.succeed("install -m 0600 ${replacementReceipt} /var/lib/korri-plugin-host/" + name + "/selection.json")
    machine.succeed("ln -sfn ${replacement} /nix/var/nix/gcroots/korri-plugin-host/" + name + "/active")
    machine.succeed("korri-plugin restore-all")
    machine.wait_for_unit(unit)
    machine.succeed("grep -F 'sleep 86400' /run/systemd/system/" + unit)
    machine.succeed("systemctl restart korri-compositor.service")
    machine.wait_for_unit(unit)
    machine.succeed("korri-plugin disable @korri:sunshine")
    machine.fail("test -e " + relation)
    for parent in ["korrid.service", "korri-compositor.service"]:
        machine.succeed("systemctl restart " + parent)
        machine.fail("systemctl is-active " + unit)
    machine.succeed("korri-plugin enable @korri:sunshine")
    machine.wait_for_unit(unit)
    machine.succeed("korri-plugin disable @korri:sunshine; korri-plugin restore-all")
    machine.fail("systemctl is-active " + unit)
    machine.fail("test -e " + relation)
  '';
}
