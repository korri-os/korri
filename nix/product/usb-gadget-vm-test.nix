# Real kernel configfs and dummy_hcd, not a directory simulation. This tests
# the product gadget module on a stock x86 kernel; it does not test any
# board's controller. Moved from the RP Mini V2 with its identity values.
{ pkgs }:
let
  identity = {
    name = "rpminiv2";
    product = "RP Mini V2 NixOS";
    address = "10.42.3.1";
    hostMac = "02:52:50:4d:32:01";
    deviceMac = "02:52:50:4d:32:02";
  };
  environment = "KORRI_USB_GADGET_NAME=${identity.name} KORRI_USB_GADGET_PRODUCT='${identity.product}' KORRI_USB_GADGET_HOST_MAC=${identity.hostMac} KORRI_USB_GADGET_DEVICE_MAC=${identity.deviceMac}";
in
pkgs.testers.runNixOSTest {
  name = "korri-product-usb-gadget";
  nodes.machine =
    { pkgs, ... }:
    {
      imports = [ ./usb-gadget.nix ];
      services.korriProduct.usbGadget = identity;
      boot.kernelModules = [
        "cdc_ncm"
        "cdc_acm"
      ];
      # Start dummy_hcd from the test, after gadget setup has started waiting.
      boot.blacklistedKernelModules = [ "dummy_hcd" ];
      networking.networkmanager.enable = true;
      services.getty.autologinUser = "root";
      environment.systemPackages = [
        pkgs.dhcpcd
        pkgs.usbutils
      ];
      virtualisation.memorySize = 1024;
    };
  testScript = ''
    start_all()
    gadget = "/sys/kernel/config/usb_gadget/rpminiv2"
    configure = "env ${environment} ${pkgs.callPackage ./usb-gadget-package.nix { }}/bin/korri-usb-gadget-configure"

    with subtest("real configfs links before delayed controller arrival"):
        machine.wait_until_succeeds(f"test -L {gadget}/configs/c.1/acm.usb0", timeout=60)
        machine.succeed("test $(stat -f -c %T /sys/kernel/config) = configfs")
        machine.succeed("test $(systemctl show usb-gadget -p ActiveState --value) = activating")
        machine.succeed(f"test -z \"$(cat {gadget}/UDC)\"")
        for function in ["ncm.usb0", "acm.usb0"]:
            machine.succeed(f"test {gadget}/configs/c.1/{function} -ef {gadget}/functions/{function}")
        machine.succeed("modprobe dummy_hcd")
        machine.wait_for_unit("usb-gadget.service")
        machine.succeed(f"test $(cat {gadget}/UDC) = dummy_udc.0")
        machine.wait_for_unit("serial-getty@ttyGS0.service")
        machine.wait_for_unit("NetworkManager.service")
        machine.wait_for_unit("systemd-networkd.service")
        machine.wait_for_unit("multi-user.target")
        machine.wait_until_succeeds("test -c /dev/ttyACM0")

    with subtest("bound invocation is idempotent and restart rebinds"):
        # Configfs refuses the original ln -sf on a bound configuration.
        machine.fail(f"cd {gadget}; ln -sf functions/ncm.usb0 configs/c.1/")
        machine.succeed(configure)
        for function in ["ncm.usb0", "acm.usb0"]:
            machine.succeed(f"test ! -e {gadget}/functions/{function}/{function}")
        machine.succeed("systemctl stop usb-gadget")
        # Regression: even after unbinding, the original command fails with
        # EEXIST. The kernel allows a function only once per configuration.
        machine.fail(f"cd {gadget}; ln -sf functions/ncm.usb0 configs/c.1/")
        machine.succeed("systemctl start usb-gadget")
        machine.succeed("systemctl restart usb-gadget")
        machine.wait_for_unit("usb-gadget.service")
        machine.succeed(f"test $(cat {gadget}/UDC) = dummy_udc.0")
        machine.wait_until_succeeds("test -c /dev/ttyACM0")
        machine.wait_for_unit("serial-getty@ttyGS0.service")

    with subtest("host DHCP and ping through NCM with the product firewall"):
        # Separate the simulated host network from the device network so ping
        # cannot pass through the local routing table instead of the USB link.
        machine.wait_until_succeeds("test -d /sys/class/net/usb1", timeout=30)
        machine.succeed("ip netns add cable; ip link set usb1 netns cable")
        machine.succeed("ip netns exec cable ip link set lo up")
        machine.succeed("ip netns exec cable dhcpcd -4 -w -t 30 --nohook resolv.conf --nohook hostname usb1")
        machine.succeed("ip -4 addr show usb0 | grep -F 10.42.3.1/24")
        machine.succeed("ip netns exec cable ip -4 addr show usb1 | grep -E 'inet 10[.]42[.]3[.]([12][0-9])/24'")
        machine.succeed("ip netns exec cable ping -c 3 -W 2 10.42.3.1")
        machine.succeed("test -z \"$(ip netns exec cable ip route show default)\"")

    with subtest("root serial shell on the same composed gadget"):
        machine.succeed("stty -F /dev/ttyACM0 raw -echo")
        machine.succeed("timeout 15 cat /dev/ttyACM0 > /tmp/usb-serial.log &")
        machine.succeed("printf '\\necho USB_ROOT_$(id -u)\\n' > /dev/ttyACM0")
        machine.wait_until_succeeds("grep -F 'USB_ROOT_0' /tmp/usb-serial.log", timeout=30)

    with subtest("stop tolerates an already-unbound gadget"):
        machine.succeed(f"echo > {gadget}/UDC")
        machine.succeed("systemctl stop usb-gadget")
        machine.succeed("systemctl start usb-gadget")
        machine.wait_for_unit("usb-gadget.service")
        machine.succeed(f"test $(cat {gadget}/UDC) = dummy_udc.0")
        machine.succeed("test $(systemctl show usb-gadget -p Result --value) = success")
        machine.succeed("systemctl --failed --no-legend | tee /tmp/failed; test ! -s /tmp/failed")
  '';
}
