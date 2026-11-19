{ pkgs }:
let
  upstream = pkgs.sway-unwrapped;
  upstreamWlroots = pkgs.wlroots_0_20;
  # A gamma reset on a CRTC with no gamma table is a no-op, not a failure.
  # Without this, the first panel power-on after idle fails on sun4i-drm
  # (RG35XX Pro; korri-os/korri#149).
  wlroots = upstreamWlroots.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [
      ./wlroots-legacy-gamma-reset.patch
      ./wlroots-libinput-touch-output.patch
    ];
  });
  patched = (upstream.override { wlroots_0_20 = wlroots; }).overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./sway-idle-notify.patch ];
  });
in
# Keep the flake's upstream source pins and all upstream dependencies/patches.
# Review the native idle boundary and the gamma reset again when upgrading.
assert upstream.version == "1.12";
assert upstream.src.outputHash == "sha256-OcF7jOOHhFPhM5APn5riy8S5jsEr9jALCVh9nBtD7Nk=";
assert upstreamWlroots.version == "0.20.2";
(pkgs.sway.override { sway-unwrapped = patched; }).overrideAttrs (old: {
  # Sway runs every `exec` command through execlp("sh", "sh", "-c", …), a
  # PATH lookup. korri-compositor.service's PATH has no shell, so the
  # compositor carries its own: without it an `exec` binding (the Tab S6
  # power key) fails with "execve failed".
  postBuild = (old.postBuild or "") + ''
    wrapProgram $out/bin/sway --suffix PATH : ${pkgs.lib.makeBinPath [ pkgs.bash ]}
  '';
  passthru = (old.passthru or { }) // {
    inherit wlroots;
  };
})
