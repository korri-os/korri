# MangoHud at the exact commit ROCKNIX builds, so the ROCKNIX patches apply
# unchanged.
#
# nixpkgs ships 0.8.2. The ROCKNIX Qualcomm and SM8550 patches target
# 992103e4 (v0.8.4) and one hunk of the Qualcomm GPU patch does not apply
# to 0.8.2. Rather than rebase a 150-line patch onto an older source, pin
# the source ROCKNIX pins. The nixpkgs recipe stays the build; only the
# source and the meson subprojects change, to the versions named in
# subprojects/*.wrap at that commit: imgui 1.91.6, implot 0.16,
# Vulkan-Headers 1.4.346, and Vulkan-Utility-Libraries 1.4.346 (new in
# 0.8.4). imgui and the two Vulkan projects use patch_directory (files
# under subprojects/packagefiles/ in the source), so only implot needs a
# wrapdb patch. Meson applies a patch_directory only when it performs the
# download itself, so the overlays are copied in by hand.
#
# Patch order matches ROCKNIX package.mk: common, then qualcomm, then
# SM8550. The zero-byte SM8550/0002-SM8750-Battery.patch is not carried.
{
  lib,
  fetchFromGitHub,
  fetchurl,
  mangohud,
}:

let
  version = "0.8.4";
  rev = "992103e4fb744897826de04ea00a2f71e7018214";

  sortedPatches =
    dir:
    map (name: dir + "/${name}") (
      lib.sort lib.lessThan (
        builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir dir))
      )
    );

  imgui = rec {
    version = "1.91.6";
    src = fetchFromGitHub {
      owner = "ocornut";
      repo = "imgui";
      tag = "v${version}";
      hash = "sha256-CLS26CRzzY4vUBgILjSQVvziHMyPGK4fwwcLZcOAzPw=";
    };
  };

  implot = rec {
    version = "0.16";
    src = fetchFromGitHub {
      owner = "epezent";
      repo = "implot";
      tag = "v${version}";
      hash = "sha256-/wkVsgz3wiUVZBCgRl2iDD6GWb+AoHN+u0aeqHHgem0=";
    };
    patch = fetchurl {
      url = "https://wrapdb.mesonbuild.com/v2/implot_${version}-1/get_patch";
      hash = "sha256-HGsUYgZqVFL6UMHaHdR/7YQfKCMpcsgtd48pYpNlaMc=";
    };
  };

  vulkan-headers = rec {
    version = "1.4.346";
    src = fetchFromGitHub {
      owner = "KhronosGroup";
      repo = "Vulkan-Headers";
      tag = "v${version}";
      hash = "sha256-JTBW5CF5hlHWkhCjjRd08hpoAarB5W3FJbHzhQM4YFs=";
    };
  };

  vulkan-utility-libraries = rec {
    version = "1.4.346";
    src = fetchFromGitHub {
      owner = "KhronosGroup";
      repo = "Vulkan-Utility-Libraries";
      tag = "v${version}";
      hash = "sha256-FWZe6NdhLmI/3bm3OIK646vkWkIQ5xmBa4jlSVHSnDs=";
    };
  };
in
mangohud.overrideAttrs (previous: {
  inherit version;

  src = fetchFromGitHub {
    owner = "flightlessmango";
    repo = "MangoHud";
    inherit rev;
    fetchSubmodules = true;
    hash = "sha256-DKmVC/YCKQp1XTdGCqZtAqoUuMhE+WUDEEETvcXbn1Y=";
  };

  postUnpack = ''
    (
      cd "$sourceRoot/subprojects"
      cp -R --no-preserve=mode,ownership ${imgui.src} imgui-${imgui.version}
      cp -R --no-preserve=mode,ownership ${implot.src} implot-${implot.version}
      cp -R --no-preserve=mode,ownership ${vulkan-headers.src} Vulkan-Headers-${vulkan-headers.version}
      cp -R --no-preserve=mode,ownership ${vulkan-utility-libraries.src} Vulkan-Utility-Libraries-${vulkan-utility-libraries.version}
    )
  '';

  # The nixpkgs postPatch substitutes bin/mangohud.in and unzips its own
  # pinned wrapdb patches. Redo the substitution and unzip only implot;
  # meson applies the packagefiles overlays for imgui and vulkan-headers
  # itself.
  postPatch = ''
    substituteInPlace bin/mangohud.in \
      --subst-var-by libraryPath ${lib.makeSearchPath "lib/mangohud" [ (placeholder "out") ]} \
      --subst-var-by version "${version}" \
      --subst-var-by dataDir ${placeholder "out"}/share

    (
      cd subprojects
      unzip ${implot.patch}
      cp -R --no-preserve=mode,ownership packagefiles/imgui-${imgui.version}/. imgui-${imgui.version}/
      cp -R --no-preserve=mode,ownership packagefiles/vulkan-headers/. Vulkan-Headers-${vulkan-headers.version}/
      cp -R --no-preserve=mode,ownership packagefiles/vulkan-utility-libraries/. Vulkan-Utility-Libraries-${vulkan-utility-libraries.version}/
    )
  '';

  patches =
    (previous.patches or [ ])
    ++ sortedPatches ./patches/common
    ++ sortedPatches ./patches/qualcomm
    ++ sortedPatches ./patches/SM8550;
})
