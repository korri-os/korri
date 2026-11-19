# Device firmware for /lib/firmware, extracted from the stock Odin firmware
# T860XXS5DWH1 without mounting anything (lz4, simg2img, debugfs, mtools).
#
# Inputs are three members of the stock tars, added to the local store by
# `nix run .#tabs6-stage-firmware -- <firmware dir>` (see README.md). They are
# fixed-output paths pinned by hash, so evaluation stays pure and the blobs
# never enter git. This derivation and its output stay on this host: they are
# proprietary, so never copy them to a binary cache.
#
#   BL  -> NON-HLOS.bin.lz4    apnhlos FAT: adsp, slpi, a640_zap, venus
#   AP  -> modem.bin.lz4       modem FAT: modem.mdt + segments, modem_pr
#   AP  -> vendor.img.ext4.lz4 sparse ext4: wlanmdsp.mbn, bdwlan.bin, crbtfw21/crnv21
#
# Plus linux-firmware: a630_sqe.fw, a640_gmu.bin, ath10k firmware-5.bin.
{
  lib,
  stdenvNoCC,
  requireFile,
  fetchFromGitHub,
  lz4,
  android-tools,
  e2fsprogs,
  mtools,
  python3,
  linux-firmware,
}:
let
  stockMember =
    name: sha256:
    requireFile {
      inherit name sha256;
      message = ''
        ${name} is a member of the stock Samsung firmware T860XXS5DWH1 for the
        SM-T860. It is proprietary and is not in the repository. Add it with:

          nix run .#tabs6-stage-firmware -- /home/simonwjackson/code/sandbox/tabs6/firmware
      '';
    };

  nonHlos = stockMember "NON-HLOS.bin.lz4" "edb16b83a030398211a947ced4ef2a866a8a23ba281fef4eec76140322532f59";
  modem = stockMember "modem.bin.lz4" "3d6d99e676196159f0b9d6b326b8c107d69aef73eb6288a5039bd80d74276ad9";
  vendor = stockMember "vendor.img.ext4.lz4" "bfb5dc2ca2308157b11c93523d74cdf6e66cd485a79eb8bf42e94e7a8eefde44";

  qcaSwissArmyKnife = fetchFromGitHub {
    owner = "qca";
    repo = "qca-swiss-army-knife";
    rev = "6df4dae3e2f5e4c2903f3cafd40996fc1b3639ce";
    hash = "sha256-FFoV58YVK7KGBw/JPFWcvGkJOONrfKCvACprYCzIKeg=";
  };

  # The remoteproc firmware directory named by the device tree
  # (firmware-name = "qcom/sm8150/gts6l/<pd>.mdt").
  board = "qcom/sm8150/gts6l";
in
stdenvNoCC.mkDerivation {
  name = "tabs6-device-firmware";
  nativeBuildInputs = [
    lz4
    android-tools
    e2fsprogs
    mtools
    python3
  ];
  dontUnpack = true;
  preferLocalBuild = true;
  allowSubstitutes = false;

  buildPhase = ''
    runHook preBuild
    export MTOOLS_SKIP_CHECK=1
    fw=$out/lib/firmware
    mkdir -p $fw/${board} $fw/qca $fw/ath10k/WCN3990/hw1.0 $fw/qcom/sm8150

    # apnhlos: ADSP, SLPI, the Samsung-signed GPU zap (GPU.md, AUDIO.md 7) and
    # the video core (VIDEO.IR.1.2, kernel/dts/sm8150-samsung-gts6lwifi-venus.dtsi).
    lz4 -dq ${nonHlos} apnhlos.bin
    for f in $(mdir -b -i apnhlos.bin ::/image | sed 's#.*/##'); do
      case "$f" in
      adsp.* | adspr.jsn | adspua.jsn | slpi.* | slpir.jsn | slpius.jsn | a640_zap.* | venus.*)
        mcopy -n -i apnhlos.bin "::/image/$f" "$fw/${board}/$f" ;;
      esac
    done

    # modem: the mpss image that hosts the WLAN firmware (WIFI.md 4).
    lz4 -dq ${modem} modem.bin
    mcopy -s -n -i modem.bin "::/image/*" "$fw/${board}/"

    # vendor: sparse ext4.
    lz4 -dq ${vendor} vendor.simg
    simg2img vendor.simg vendor.ext4
    rm vendor.simg
    get() { debugfs -R "dump -p $1 $2" vendor.ext4; test -s "$2"; }
    get /firmware/wlanmdsp.mbn $fw/${board}/wlanmdsp.mbn
    cp $fw/${board}/wlanmdsp.mbn $fw/wlanmdsp.mbn
    get /firmware/wlan/qca_cld/bdwlan.bin bdwlan.bin
    # hci_qca asks for the "01" names on this ROM; the device ships "21"
    # (BLUETOOTH.md 3).
    get /firmware/crbtfw21.tlv $fw/qca/crbtfw21.tlv
    get /firmware/crnv21.bin $fw/qca/crnv21.bin
    cp $fw/qca/crbtfw21.tlv $fw/qca/crbtfw01.tlv
    cp $fw/qca/crnv21.bin $fw/qca/crnv01.bin
    rm vendor.ext4

    # ath10k board data from this device's own calibration, under the names
    # ath10k asks for (WIFI.md 5).
    cat > board-2.json <<'EOF'
    [
      {
        "names": [
          "bus=snoc,qmi-board-id=ff,qmi-chip-id=30224",
          "bus=snoc,qmi-board-id=ff"
        ],
        "data": "bdwlan.bin"
      }
    ]
    EOF
    python3 ${qcaSwissArmyKnife}/tools/scripts/ath10k/ath10k-bdencoder \
      --create board-2.json -o $fw/ath10k/WCN3990/hw1.0/board-2.bin
    python3 ${qcaSwissArmyKnife}/tools/scripts/ath10k/ath10k-bdencoder \
      --info $fw/ath10k/WCN3990/hw1.0/board-2.bin

    # Redistributable pieces, uncompressed (no FW_LOADER_COMPRESS assumed).
    lf=${linux-firmware}/lib/firmware
    cp $lf/ath10k/WCN3990/hw1.0/firmware-5.bin $fw/ath10k/WCN3990/hw1.0/
    cp $lf/qcom/a630_sqe.fw $fw/qcom/
    cp $lf/qcom/a640_gmu.bin $fw/qcom/
    cp $lf/qcom/a640_gmu.bin $fw/qcom/sm8150/

    # GPU.md stages the zap under gts6lwifi; the board DTS names gts6l.
    ln -s gts6l $fw/qcom/sm8150/gts6lwifi

    chmod -R u+w,a+r $fw
    runHook postBuild
  '';

  doCheck = true;
  checkPhase = ''
    fw=$out/lib/firmware
    for f in ${board}/modem.mdt ${board}/adsp.mdt ${board}/slpi.mdt ${board}/a640_zap.mdt ${board}/venus.mdt ${board}/venus.b02 \
      ${board}/wlanmdsp.mbn wlanmdsp.mbn qca/crbtfw01.tlv qca/crnv01.bin \
      ath10k/WCN3990/hw1.0/board-2.bin ath10k/WCN3990/hw1.0/firmware-5.bin \
      qcom/a630_sqe.fw qcom/a640_gmu.bin; do
      test -s "$fw/$f" || { echo "missing $f"; exit 1; }
    done
  '';
  dontInstall = true;
  # Signed blobs must stay byte-identical: no patchelf, strip or shebang fixes.
  dontFixup = true;

  meta = {
    description = "Galaxy Tab S6 (SM-T860) device firmware from the stock T860XXS5DWH1 image";
    license = lib.licenses.unfree;
    platforms = lib.platforms.all;
  };
}
