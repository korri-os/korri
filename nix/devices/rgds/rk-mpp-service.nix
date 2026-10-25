# Linux 7.2 rk3568-anbernic-rg-ds.dts identifies this board as anbernic,rg-ds.
# RK3566 and RK3568 use the same RKVENC-v1 binding; change only board matching.
import ../rockchip/rk-mpp-service.nix {
  device = "rgds";
  overlay = builtins.toFile "rgds-rk-mpp-service-overlay.dts" (
    builtins.replaceStrings
      [ ''compatible = "anbernic,rg353p";'' ]
      [ ''compatible = "anbernic,rg-ds";'' ]
      (builtins.readFile ../rockchip/rk-mpp-service-overlay.dts)
  );
}
