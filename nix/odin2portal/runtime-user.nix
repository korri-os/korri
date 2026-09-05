# Non-root userspace identity for device services.
#
# The recovery console remains root. Lingering starts this user's systemd
# manager at boot, so PipeWire and WirePlumber can run without greetd or an
# interactive login. The audio group is necessary because this boot-only
# user manager has no interactive logind seat ACL.
{
  users.groups.korri.gid = 1000;
  users.users.korri = {
    isNormalUser = true;
    uid = 1000;
    group = "korri";
    home = "/home/korri";
    createHome = true;
    linger = true;
    extraGroups = [ "audio" ];
  };
}
