rec {
  baseSunshineVersion = "2025.924.154138";
  approvedBaseSourceHash = "sha256-QrPfZqd9pgufohUjxlTpO6V0v7B41UrXHZaESsFjZ48=";
  approvedDeviceBaseDerivation = "/nix/store/5hmg1ff3wjkzcmspssckdlydk0d5bbjz-sunshine-2025.924.154138.drv";
  approvedDeviceBaseDerivations = {
    "x86_64-linux-cuda" = "/nix/store/5hmg1ff3wjkzcmspssckdlydk0d5bbjz-sunshine-2025.924.154138.drv";
    "aarch64-linux-software" =
      "/nix/store/8dhfxx3xi04qvlv8ihrg4p6ycqwx6fhc-sunshine-2025.924.154138.drv";
    "aarch64-linux-rkmpp" = "/nix/store/8dhfxx3xi04qvlv8ihrg4p6ycqwx6fhc-sunshine-2025.924.154138.drv";
  };
  approvedBaseDerivationsByProfile = {
    "x86_64-linux-cuda" = [
      # Korri nixpkgs revision a6531044f6d0bef691ea18d4d4ce44d0daa6e816.
      "/nix/store/63c39c0mjs72ixh20hs18r8l8zh3wix7-sunshine-2025.924.154138.drv"
      # Mountainous nixpkgs revision c06b4ae3d6599a672a6210b7021d699c351eebda.
      "/nix/store/5hmg1ff3wjkzcmspssckdlydk0d5bbjz-sunshine-2025.924.154138.drv"
    ];
    "aarch64-linux-software" = [
      # Korri nixpkgs revision a6531044f6d0bef691ea18d4d4ce44d0daa6e816.
      "/nix/store/8dhfxx3xi04qvlv8ihrg4p6ycqwx6fhc-sunshine-2025.924.154138.drv"
    ];
    "aarch64-linux-rkmpp" = [
      # Same reviewed CUDA-free upstream recipe; RKMPP is added only downstream.
      "/nix/store/8dhfxx3xi04qvlv8ihrg4p6ycqwx6fhc-sunshine-2025.924.154138.drv"
    ];
  };
  approvedBaseDerivations = builtins.concatLists (
    builtins.attrValues approvedBaseDerivationsByProfile
  );
  reviewedLibavcodecVersion = "62.11.100";
  reviewedFfmpegCommit = "61c50407fd429a5e2ec616e2e846c3fe3743879a";
  reviewedFfmpegSourceHash = "sha256-LKQUfHb9/Z4uvPx4vrtAOPL95Un9/C26lvCbQZ51avk=";
  reviewedBuildDepsCommit = "2851db101eeddae8f02489d48a52a4d83e6f7e7b";
  reviewedBuildDepsSourceHash = "sha256-ojpcgvn2DItXQp1lqrL4eVdv0MXwcAo0eGfcqzZQvz4=";
  v4l2m2mPatches = [
    {
      name = "0001-fix-v4l2m2m-buffer-alignment.patch";
      path = ./patches/ffmpeg/0001-fix-v4l2m2m-buffer-alignment.patch;
      sha256 = "11f484533ca7cc2296c67d145fc95cc77dda4e99ac601cdec8e8682db2b1856f";
      upstreamPullRequest = "https://code.ffmpeg.org/FFmpeg/FFmpeg/pulls/24328";
      upstreamCommit = "3fda94e1309bead4d39ea4b2cc42d13f8cdf48b4";
      sourceArchive = "https://www.mail-archive.com/ffmpeg-devel@ffmpeg.org/msg190438.html";
    }
    {
      name = "0002-add-v4l2m2m-repeat-headers.patch";
      path = ./patches/ffmpeg/0002-add-v4l2m2m-repeat-headers.patch;
      sha256 = "80830fb6bb168281d14ff537c9691b0bfb036db53bdb01d47f72b16b2847666c";
    }
  ];
  v4l2m2mPatchSetSha256 = "e918232613be4264d0c7c55900ff030674cc66e3f8f37f4daa0c59f1051960a6";
  reviewedNvencApiMajor = 12;
  reviewedNvencApiMinor = 0;
  patchSetSha256 = "b009201c4e2b09dd22d24e895a831142e9b6070446cfe3a89ee0a0a22bc9a75c";
  patches = [
    {
      name = "0001-add-runtime-settings-protocol-surface.patch";
      path = ./patches/0001-add-runtime-settings-protocol-surface.patch;
      sha256 = "8a9522e39de85cb4ea7c0558a806780ae39d588555c7a84c600a56b9fdbe3bd4";
    }
    {
      name = "0002-wire-runtime-settings-control-plane.patch";
      path = ./patches/0002-wire-runtime-settings-control-plane.patch;
      sha256 = "dd9b7283dd2cbcb2476571bfcf61702b00dba428422d309e31e7b4c839db41be";
    }
    {
      name = "0003-apply-runtime-bitrate-and-fps-changes.patch";
      path = ./patches/0003-apply-runtime-bitrate-and-fps-changes.patch;
      sha256 = "d7d89d4a8b4b06d2c473f4c2156a17ecfe369f805132e90a2d05197e69e7e01d";
    }
    {
      name = "0004-add-proof-gated-runtime-resolution-apply-path.patch";
      path = ./patches/0004-add-proof-gated-runtime-resolution-apply-path.patch;
      sha256 = "599d3db14ea57e9712148e83fd7f0404dba96c5c40506c3209c5dbaa7778646e";
    }
    {
      name = "0005-add-seamless-vaapi-runtime-bitrate-path.patch";
      path = ./patches/0005-add-seamless-vaapi-runtime-bitrate-path.patch;
      sha256 = "a14ca9d556728ca1a4fcb14ae338a6275c9b28c52598a82a4e4f424956154d53";
    }
    {
      name = "0010-extend-runtime-resolution-fresh-idr-window.patch";
      path = ./patches/0010-extend-runtime-resolution-fresh-idr-window.patch;
      sha256 = "86252208da87bff0b61623f7da86e50d9f35c19963910e5e30703b72b86a42eb";
    }
    {
      name = "0012-persist-runtime-config-and-reinit-capture-after-resolution.patch";
      path = ./patches/0012-persist-runtime-config-and-reinit-capture-after-resolution.patch;
      sha256 = "2ac28eb76da2d02aa97812e9708094480cc1b7c4b897cf123772c24f16c493c6";
    }
    {
      name = "0013-request-async-capture-reinit-after-runtime-resolution.patch";
      path = ./patches/0013-request-async-capture-reinit-after-runtime-resolution.patch;
      sha256 = "0831530081f9551173ff1a74a5ca2771942e9c519ec476c27548a1d3cbea3fa2";
    }
    {
      name = "0014-skip-runtime-vaapi-destructor-flush.patch";
      path = ./patches/0014-skip-runtime-vaapi-destructor-flush.patch;
      sha256 = "59eedaf576f99223bd807205c45b12b1ac5f9850225614530b4ab925e3204e50";
    }
    {
      name = "0015-add-korri-input-seat-event-mirror.patch";
      path = ./patches/0015-add-korri-input-seat-event-mirror.patch;
      sha256 = "c0eab65a69c17b2f3f5b6c9c69fabaaf51519da26a81f9e57dbd714ab5ca92db";
    }
    {
      name = "0016-add-seamless-nvenc-runtime-path.patch";
      path = ./patches/0016-add-seamless-nvenc-runtime-path.patch;
      sha256 = "686decb81379741e01e0b9b0e9105bbe23765a1bf728565767604383983a7074";
    }
    {
      name = "0017-use-wayland-ram-capture-for-cuda.patch";
      path = ./patches/0017-use-wayland-ram-capture-for-cuda.patch;
      sha256 = "a87aefc6eb5f71a4d413d751eefb87743745a2fab126dded5b66b23b949f66b2";
    }
    {
      name = "0018-vectorize-wayland-bgr888-with-swscale.patch";
      path = ./patches/0018-vectorize-wayland-bgr888-with-swscale.patch;
      sha256 = "753971f16e33598215caa455074f3bbca23e43b0cf2a2b8a97779f356486203f";
    }
    {
      name = "0019-use-pinned-memory-for-cuda-capture.patch";
      path = ./patches/0019-use-pinned-memory-for-cuda-capture.patch;
      sha256 = "83fd586d210668b06753fdd8bb6312967ba8805ce7a2fdc992c5cbdc49c79c88";
    }
    {
      name = "0020-add-korrid-certificate-control.patch";
      path = ./patches/0020-add-korrid-certificate-control.patch;
      sha256 = "8e97eb5c8cf30a5b80b6a13aa88102de4ff443103875873ae4eb8f483ccfe059";
    }
    {
      name = "0021-add-v4l2m2m-encoder.patch";
      path = ./patches/0021-add-v4l2m2m-encoder.patch;
      sha256 = "64e51b7085e2678d2abb04aafb5d9a4c8d961a2f2b6ced7c7636f85ec67a66b3";
    }
  ];

  # Applied after every base patch, only for the aarch64-linux-rkmpp profile.
  # It adds the Rockchip MPP H.264 encoder definition and paces its encode
  # loop to the negotiated frame rate.
  rkmppPatch = {
    name = "0022-add-rkmpp-h264-encoder.patch";
    path = ./patches/0022-add-rkmpp-h264-encoder.patch;
    sha256 = "36d8f96b01b68e9b4cbad4edc7f03d46b64eb5e291d116fe57470398d846b054";
  };

  # Applied after the RKMPP encoder patch, only for the
  # aarch64-linux-rkmpp profile. It hands compatible KMS scanout dma-bufs
  # directly to h264_rkmpp as DRM_PRIME frames.
  rkmppZeroCopyPatch = {
    name = "0023-add-rkmpp-drm-prime-zero-copy.patch";
    path = ./patches/0023-add-rkmpp-drm-prime-zero-copy.patch;
    sha256 = "601a6ce16580902dae652920f4af819fa19afb7aa430b1c65860b3c8f99b9086";
  };

  # Scope the stdin pairing fix to the device profile while validating it;
  # leave the approved x86 build unchanged.
  stdinPinPatch = {
    name = "0024-fix-stdin-pin-pairing-return.patch";
    path = ./patches/0024-fix-stdin-pin-pairing-return.patch;
    sha256 = "eeab5c78eea1c2c1f72277fb370681b5352b28da75a58668bfda043836b28020";
  };

  rkmppPatches = [
    rkmppPatch
    rkmppZeroCopyPatch
    stdinPinPatch
  ];

  # Ordered digest of patches ++ rkmppPatches. Bump only after reviewing the
  # complete base-plus-RKMPP patch order.
  rkmppPatchSetSha256 = "287ff6ef2a6789d4abfd73dc05b839384b17a71b0a45d768e3764626e553b5c6";
}
