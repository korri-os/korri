{
  pkgs,
  rocknixArchive ? null,
}:
let
  archive =
    if rocknixArchive != null then
      rocknixArchive
    else
      pkgs.fetchurl {
        url = "https://github.com/ROCKNIX/distribution/releases/download/20260901/ROCKNIX-SM8250.aarch64-20260901.img.gz";
        sha256 = "3a4bf87ff2a45f60d6d4bc2c67177fd8e9c3e974805a7579b34d9fcd68c9667d";
      };
in
pkgs.runCommand "rpminiv2-rocknix-20260901-baseline"
  {
    nativeBuildInputs = [
      pkgs.gzip
      pkgs.mtools
    ];
    # This output extracts files from a third-party system image. Keep it local
    # until redistribution terms for the complete image have been reviewed.
    preferLocalBuild = true;
    allowSubstitutes = false;
  }
  ''
    image="$TMPDIR/rocknix-sm8250.img"
    gzip -dc ${archive} > "$image"

    mkdir -p "$out"
    export MTOOLSRC=/dev/null
    mcopy -i "$image@@16777216" ::/EFI/BOOT/bootaa64.efi "$out/bootaa64.efi"
    mcopy -i "$image@@16777216" ::/boot/grub/dejavu-mono.pf2 "$out/dejavu-mono.pf2"
    mcopy -i "$image@@16777216" ::/boot/grub/grub.cfg "$out/rocknix-grub.cfg"
    mcopy -i "$image@@16777216" \
      ::/boot/grub/sm8250-retroidpocket-rpminiv2.dtb \
      "$out/sm8250-retroidpocket-rpminiv2.dtb"

    cat > checksums <<'EOF'
    39de9119311fa4274f27908a561f2b876133810325d9deff89a9461f832c838b  bootaa64.efi
    734f45a5b8c134b5cc161d02a9650fa7cf939abbab1a32f254bab8da201c9385  dejavu-mono.pf2
    efa07a2c290a2ae82ededc3ed226be27b96b010187e4d16f6306fb757effba88  rocknix-grub.cfg
    f9e32c33e14f3d974c461c674435a7002c73ec4243e96e3aef158620a730eee4  sm8250-retroidpocket-rpminiv2.dtb
    EOF
    (cd "$out" && sha256sum -c "$NIX_BUILD_TOP/checksums")
    cp checksums "$out/SHA256SUMS"
  ''
