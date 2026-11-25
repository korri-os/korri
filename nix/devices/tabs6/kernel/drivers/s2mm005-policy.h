/* SPDX-License-Identifier: GPL-2.0 */
#ifndef S2MM005_POLICY_H
#define S2MM005_POLICY_H

/* FUNC_STATE bytes 0/3, MUIC RSVDID1, charger CNTL.
 * Samsung 37cf30da: s2mm005.h FUNC_STATE_Type, muic_regmap_sm5705.c,
 * sm5705.h. See usb-role/typec.md. No field grants permission to source VBUS.
 * No fuel-gauge input: this board sources VBUS only in SM5705 mode 7, so
 * with CNTL mode 5, VBUS=01 + SNK_Ready/source=0 prove the charger supplies
 * the port. The gauge flips Full/Discharging around its 50 mA deadband at
 * 100 % with the charger in (usb-role/sink-host-full-discharging-dips.txt).
 * bool is supplied by linux/types.h or stdbool.h in the host test.
 */
struct s2mm005_sample {
	unsigned char state;
	unsigned char flags;
	unsigned char muic_vbus;
	unsigned char cntl;
};

/* NULL means any read failed. Every call must consume a new hardware sample.
 * Only enables wait; revocations clear qualification on the same call.
 */
static inline bool s2mm005_sink_host(unsigned int *consecutive,
				   const struct s2mm005_sample *s)
{
	if (!s || s->state != 21 || (s->flags & 7) != 5 ||
	    s->muic_vbus != 1 || (s->cntl & 7) != 5) {
		*consecutive = 0;
		return false;
	}
	if (*consecutive < 3)
		++*consecutive;
	return *consecutive == 3;
}
#endif
