# The Korri Plymouth theme. Its images are rendered here from the brand SVGs,
# so the splash can never drift from the mark; nothing is checked in.
{
  lib,
  stdenvNoCC,
  python3,
  resvg,
  # Match the panel: RG353M 60 Hz, Odin 2 Portal 120 Hz. Plymouth's script
  # plugin defaults to 50, which beats against both.
  refreshRate ? 60,
}:
stdenvNoCC.mkDerivation {
  pname = "plymouth-theme-korri";
  version = "0.0.0";

  src = lib.fileset.toSource {
    root = ./..;
    fileset = lib.fileset.unions [
      ./korri.script
      ./split-wordmark.py
      ../korri-mark.svg
      ../korri-wordmark-dark.svg
    ];
  };

  nativeBuildInputs = [
    python3
    resvg
  ];

  buildPhase = ''
    runHook preBuild
    mkdir -p theme
    python3 plymouth/split-wordmark.py . theme ${lib.getExe' resvg "resvg"}
    # geometry.script holds the measured numbers; korri.script is the animation.
    cat theme/geometry.script > theme/korri.script
    echo "global.REFRESH_RATE = ${toString refreshRate};" >> theme/korri.script
    cat plymouth/korri.script >> theme/korri.script
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    dir="$out/share/plymouth/themes/korri"
    mkdir -p "$dir"
    cp theme/leaf.png theme/glyph-*.png theme/korri.script "$dir/"
    cat > "$dir/korri.plymouth" <<EOF
    [Plymouth Theme]
    Name=Korri
    Description=The Korri leaf sprouts, then becomes the wordmark.
    ModuleName=script

    [script]
    ImageDir=$dir
    ScriptFile=$dir/korri.script
    EOF
    sed -i 's/^    //' "$dir/korri.plymouth"
    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    dir="$out/share/plymouth/themes/korri"
    test -s "$dir/leaf.png"
    test -s "$dir/glyph-4.png"
    grep -q '^ScriptFile=' "$dir/korri.plymouth"
    grep -q '^global.LEAF_PET_X' "$dir/korri.script"
    grep -q "^global.REFRESH_RATE = ${toString refreshRate};" "$dir/korri.script"
    runHook postInstallCheck
  '';

  meta.description = "Korri boot splash for Plymouth";
}
