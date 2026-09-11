# Both observed ALSA sinks have priority.session=1000, so disconnected HDMI can
# win on boot. Prefer outputs on the onboard RK817 card without changing UCM
# headphone routing, microphones, volumes or HDMI's explicit availability.
{ ... }:
{
  services.pipewire.wireplumber.extraConfig."51-rg353m-default-audio" = {
    "monitor.alsa.rules" = [
      {
        matches = [ { "node.name" = "~alsa_output[.]platform-sound[.].*"; } ];
        actions."update-props"."priority.session" = 1100;
      }
    ];
  };
}
