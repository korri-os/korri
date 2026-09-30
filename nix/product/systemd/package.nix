# Product stop semantics, not an upstream bugfix-only backport.
{ systemd }:
assert systemd.version == "258.2";
systemd.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./stop-thaws-unit.patch ];
})
