{
  shell,
  system,
  text,
  preferLocalBuild ? false,
  allowSubstitutes ? true,
}:
builtins.derivation {
  name = "korri-device-cache-fixture";
  inherit
    system
    text
    preferLocalBuild
    allowSubstitutes
    ;
  builder = shell;
  args = [
    "-c"
    ''printf '%s\n' "$text" > "$out"''
  ];
}
