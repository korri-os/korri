// SPDX-License-Identifier: GPL-2.0-only
/*
 * Runtime-only loader for the RG353M RKVENC device-tree overlay.
 *
 * This is a bring-up aid: it lets the encoder and IOMMU nodes be tested on the
 * running kernel without replacing the boot DTB. Production boots receive the
 * same overlay through NixOS hardware.deviceTree.overlays.
 *
 * Deliberately omit a module exit function. Mainline's Rockchip IOMMU creates
 * device links for the dynamically added nodes, and removing those nodes after
 * use can corrupt the device-link list. Reboot to remove this bring-up overlay.
 */
#include <linux/module.h>
#include <linux/of.h>

#include "rk_mpp_overlay_blob.h"

static int __init rk_mpp_overlay_init(void)
{
	int overlay_id = -1;
	int ret;

	ret = of_overlay_fdt_apply(rk_mpp_overlay_dtbo,
				   rk_mpp_overlay_dtbo_len,
				   &overlay_id, NULL);
	if (ret) {
		pr_err("rk_mpp_overlay: apply failed: %d (id %d)\n",
		       ret, overlay_id);
		return ret;
	}

	pr_info("rk_mpp_overlay: applied overlay id %d; reboot to remove\n",
		overlay_id);
	return 0;
}

module_init(rk_mpp_overlay_init);

MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Runtime loader for the RG353M RKVENC MPP DT overlay");
