# Check the product's wiring, then exercise the shared profile with NixOS's real
# service and NetworkManager. Credentials exist only inside the test VM.
{ pkgs, configuration }:
let
  environmentFiles =
    configuration.config.systemd.services.NetworkManager-ensure-profiles.serviceConfig.EnvironmentFile;
in
assert environmentFiles == [ "-/etc/korri/wifi.env" ];
pkgs.testers.runNixOSTest {
  name = "rpminiv2-optional-wifi-environment";
  nodes.machine = { ... }: {
    imports = [ ../../base/wifi.nix ];
    networking.networkmanager.enable = true;
  };
  testScript = ''
    import shlex

    machine.start()
    machine.wait_for_unit("NetworkManager.service")
    machine.succeed("systemctl start NetworkManager-ensure-profiles")
    machine.fail("test -e /etc/korri/wifi.env")
    machine.succeed("test $(systemctl show -p Result --value NetworkManager-ensure-profiles) = success")

    # Restore a persistent profile through NM's native producer. No physical
    # Wi-Fi hardware is needed to test profile preservation or service startup.
    machine.succeed("nmcli connection add type wifi con-name korri ifname wlan0 ssid restored-test")
    before = machine.succeed("sha256sum /etc/NetworkManager/system-connections/*")
    uuid = machine.succeed("nmcli -g connection.uuid connection show korri").strip()
    machine.shutdown()
    machine.start()
    machine.wait_for_unit("NetworkManager.service")
    machine.fail("test -e /etc/korri/wifi.env")
    machine.succeed("systemctl restart NetworkManager-ensure-profiles")
    assert machine.succeed("sha256sum /etc/NetworkManager/system-connections/*") == before
    machine.succeed("nmcli connection show " + shlex.quote(uuid))
    machine.succeed("test $(systemctl show -p Result --value NetworkManager-ensure-profiles) = success")

    # Provisioning still works when the optional file is supplied. Generate the
    # throwaway PSK at runtime rather than embedding credentials in the store.
    machine.succeed("install -d -m 700 /etc/korri; umask 077; printf 'WIFI_SSID=provisioned-test\\nWIFI_PSK=' > /etc/korri/wifi.env; tr -d '-' < /proc/sys/kernel/random/uuid >> /etc/korri/wifi.env")
    machine.succeed("systemctl restart NetworkManager-ensure-profiles")
    machine.succeed("test $(systemctl show -p Result --value NetworkManager-ensure-profiles) = success")
    machine.succeed("grep -q '^ssid=provisioned-test$' /run/NetworkManager/system-connections/korri.nmconnection")
    assert machine.succeed("sha256sum /etc/NetworkManager/system-connections/*") == before
    machine.succeed("nmcli connection show " + shlex.quote(uuid))
  '';
}
