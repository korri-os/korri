# The product pins Linux 7.2 (owner decision, 2026-09-30). A device may run a
# different kernel series only when it is necessary, and it states why.
#
# The pin is the kernel series. Which 7.2 build a board uses (ROCKNIX-derived,
# mainline plus config, a point release) stays a hardware fact.
{ config, lib, ... }:
let
  cfg = config.services.korriProduct.kernel;
  actual = lib.versions.majorMinor config.boot.kernelPackages.kernel.version;
in
{
  options.services.korriProduct.kernel = {
    series = lib.mkOption {
      type = lib.types.strMatching "^[0-9]+\\.[0-9]+$";
      default = "7.2";
      readOnly = true;
      description = "Linux series every product device runs. A device states an override reason instead of changing this.";
    };
    overrideReason = lib.mkOption {
      type = lib.types.nullOr lib.types.nonEmptyStr;
      default = null;
      description = "Why this device must run another kernel series. Required when the kernel is not the product series.";
    };
    pinHolds = lib.mkOption {
      type = lib.types.bool;
      readOnly = true;
      internal = true;
      default = actual == cfg.series || cfg.overrideReason != null;
      description = "Whether this configuration meets the kernel pin.";
    };
  };

  config.assertions = [
    {
      assertion = cfg.pinHolds;
      message = "Korri product devices run Linux ${cfg.series}; this configuration runs ${actual}. Set services.korriProduct.kernel.overrideReason only when another series is necessary.";
    }
  ];
}
