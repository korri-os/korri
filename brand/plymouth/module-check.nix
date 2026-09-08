# Boot a VM with the Korri splash and photograph what Plymouth actually draws.
#
# This test exists because the script plugin fails silently: a bad theme logs
# nothing and simply leaves the screen black. It has already caught three
# faults that a build cannot see —
#   * Math.Pi () returned null, so every rotated leaf was a null image
#   * a serial console on the cmdline forced the details plugin
#   * consoleLogLevel conflicted with the device image's own setting
# so the assertions are about pixels, not about units starting.
{ pkgs, nixpkgs }:
pkgs.testers.runNixOSTest {
  name = "korri-boot-splash";
  nodes.machine = {
    imports = [ ./nixos-module.nix ];
    services.korri.bootSplash = {
      enable = true;
      quiet = true;
    };
    # Hold the splash open long enough to photograph it. Without this, boot
    # finishes and Plymouth quits before the test can look.
    systemd.services.korri-boot-splash-handoff.serviceConfig.ExecStartPre = "${pkgs.coreutils}/bin/sleep 30";
    # plymouthd picks its renderer at startup; with no DRM device yet it falls
    # back to the details plugin and the theme never draws.
    boot.initrd.kernelModules = [
      "virtio_gpu"
      "bochs"
    ];
    virtualisation.qemu.options = [ "-vga virtio" ];
  };
  testScript = ''
    import struct, zlib

    def colours(name):
        """Distinct colours in a screenshot, read without leaving the sandbox."""
        path = f"{machine.out_dir}/{name}.png"
        raw, seen = open(path, "rb").read(), set()
        pos, idat = 8, b""
        while pos < len(raw):
            ln, typ = struct.unpack(">I", raw[pos:pos + 4])[0], raw[pos + 4:pos + 8]
            if typ == b"IHDR":
                w, h, depth, mode = struct.unpack(">IIBB", raw[pos + 8:pos + 18])
            elif typ == b"IDAT":
                idat += raw[pos + 8:pos + 8 + ln]
            pos += 12 + ln
        assert mode == 2 and depth == 8, f"expected 8-bit truecolour, got mode {mode} depth {depth}"
        data, stride = zlib.decompress(idat), w * 3
        prev, out = bytearray(stride), bytearray()
        at = 0
        for _ in range(h):
            f, line = data[at], bytearray(data[at + 1:at + 1 + stride])
            at += 1 + stride
            for i in range(stride):
                a = line[i - 3] if i >= 3 else 0
                b = prev[i]
                c = prev[i - 3] if i >= 3 else 0
                if f == 1: line[i] = (line[i] + a) & 255
                elif f == 2: line[i] = (line[i] + b) & 255
                elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
                elif f == 4:
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
            out += line
            prev = line
        for i in range(0, len(out), 3):
            seen.add(bytes(out[i:i + 3]))
        return seen

    GREEN = (0x7E, 0xFD, 0x3B)

    def has_leaf(name):
        """True when the brand green is on screen, allowing for scaling."""
        for r, g, b in colours(name):
            if abs(r - GREEN[0]) < 40 and abs(g - GREEN[1]) < 40 and abs(b - GREEN[2]) < 40:
                return True
        return False

    machine.start()
    machine.sleep(8)
    machine.screenshot("splash-sway")
    machine.succeed("plymouth --ping")
    assert has_leaf("splash-sway"), "the leaf never drew: Plymouth showed no brand green"

    machine.succeed("plymouth display-message --text=korri:wordmark")
    machine.sleep(2)
    machine.screenshot("splash-wordmark")
    wordmark = colours("splash-wordmark")
    assert has_leaf("splash-wordmark"), "the leaf vanished when the wordmark assembled"
    white = [c for c in wordmark if min(c) > 0xE0]
    assert white, "the wordmark glyphs never appeared"

    machine.wait_for_unit("multi-user.target")
    machine.succeed("journalctl -b -u korri-boot-splash-handoff | grep -q 'Finished'")
  '';
}
