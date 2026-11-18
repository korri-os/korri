// SPDX-License-Identifier: GPL-2.0-only
/*
 * Samsung AMSA05RB06 AMOLED panel with an Anapass ANA38401 T-CON, as fitted
 * to the Galaxy Tab S6 (SM-T860, gts6lwifi): 2560x1600, two bonded DSI links
 * of 1280x1600 each, command mode, no DSC.
 *
 * Everything here comes from the stock T860XXS5DWH1 firmware:
 *  - the panel node ss_dsi_panel_ANA38401_AMSA05RB06_WQXGA in the dtbo
 *    (on/off command sets, timings, reset sequence, delays), and
 *  - Samsung's kernel source for that release
 *    (drivers/gpu/drm/msm/samsung/ANA38401_AMSA05RB06/, dsi-staging/dsi_panel.c)
 *    for the power sequence and the AOR/ELVSS brightness table.
 *
 * Vendor behaviour this driver keeps on purpose:
 *  - Power sequence "VDD on -> LP11 -> reset -> wait for T-CON ready -> HS"
 *    (samsung,anapass-power-seq). prepare_prev_first gives LP11 before reset.
 *  - Every command goes to the first link only (samsung,cmds-unicast) and in
 *    its own transfer, with the packet type the vendor uses. The T-CON takes
 *    the 2-byte "b2" parameter writes as DCS long writes, which the generic
 *    mipi_dsi_dcs_write helpers would send as short ones.
 *  - Display on is sent no earlier than 220 ms after sleep out
 *    (samsung,wait_after_sleep_out_delay).
 *
 * Not vendor: the vendor on-sequence has no sleep-out command, but without
 * one the T-CON stays in sleep in (0x0a reads 0x04 after display on) and the
 * panel stays black. Exit sleep (0x11) after the on-sequence lights it.
 *
 * Brightness: the vendor driver sets AOR (0x90), ELVSS and a gamma table it
 * computes from per-panel MTP data (smart dimming). This driver sends the
 * vendor AOR/ELVSS table and keeps the neutral gamma the on-sequence writes,
 * so luminance follows the AOR duty cycle. Colour at low levels can differ
 * from stock; the brightness range is the same.
 */

#include <linux/backlight.h>
#include <linux/delay.h>
#include <linux/devm-helpers.h>
#include <linux/gpio/consumer.h>
#include <linux/iopoll.h>
#include <linux/ktime.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/of.h>
#include <linux/of_graph.h>
#include <linux/regulator/consumer.h>
#include <linux/slab.h>
#include <linux/workqueue.h>

#include <video/mipi_display.h>

#include <drm/drm_mipi_dsi.h>
#include <drm/drm_modes.h>
#include <drm/drm_panel.h>
#include <drm/drm_probe_helper.h>

/* qcom,mdss-dsi-reset-sequence = <0 70 1 0>: held in reset for 70 ms. */
#define ANA38401_RESET_MS		70
/* qcom,supply-post-on-sleep / qcom,supply-pre-off-sleep for vddlcd. */
#define ANA38401_VDD_ON_MS		10
#define ANA38401_VDD_OFF_MS		5
/*
 * The vendor polls T-CON ready for 100 ms. A sister Anapass T-CON (Tab S8+)
 * takes about 330 ms, so allow more before giving up and carrying on, as the
 * vendor does.
 */
#define ANA38401_TCON_RDY_TIMEOUT_US	500000
/* samsung,wait_after_sleep_out_delay = <220>. */
#define ANA38401_DISPLAY_ON_DELAY_MS	220
/* display_off_tx_cmds: wait byte 0x64 after 0x28. */
#define ANA38401_DISPLAY_OFF_MS		100

struct ana38401 {
	struct drm_panel panel;
	struct mipi_dsi_device *dsi[2];
	struct regulator *vdd;
	struct gpio_desc *reset_gpio;
	struct gpio_desc *tcon_rdy_gpio;
	struct backlight_device *backlight;
	/* Serialises command transfers between the panel and backlight paths. */
	struct mutex lock;
	bool on;
	ktime_t reset_time;
	ktime_t sleep_out_time;
	/* Bring-up diagnostics: a status read once frames have flowed. */
	struct delayed_work status_work;
};

static inline struct ana38401 *to_ana38401(struct drm_panel *panel)
{
	return container_of(panel, struct ana38401, panel);
}

/*
 * One vendor command: DSI packet type plus payload, sent on the first link in
 * high-speed mode (on/off-command-state = dsi_hs_mode).
 */
static void ana38401_tx(struct mipi_dsi_multi_context *dsi_ctx, u8 type,
			const u8 *data, size_t len)
{
	struct mipi_dsi_device *dsi = dsi_ctx->dsi;
	struct mipi_dsi_msg msg = {
		.channel = dsi->channel,
		.type = type,
		.tx_buf = data,
		.tx_len = len,
	};
	ssize_t ret;

	if (dsi_ctx->accum_err)
		return;

	if (dsi->mode_flags & MIPI_DSI_MODE_LPM)
		msg.flags |= MIPI_DSI_MSG_USE_LPM;

	ret = dsi->host->ops->transfer(dsi->host, &msg);
	if (ret < 0) {
		dsi_ctx->accum_err = ret;
		dev_err(&dsi->dev, "command %02x (type %02x) failed: %zd\n",
			data[0], type, ret);
	}
}

#define ana38401_short(ctx, seq...) do {				\
		static const u8 d[] = { seq };				\
		ana38401_tx(ctx, MIPI_DSI_DCS_SHORT_WRITE_PARAM, d, sizeof(d)); \
	} while (0)

#define ana38401_long(ctx, seq...) do {					\
		static const u8 d[] = { seq };				\
		ana38401_tx(ctx, MIPI_DSI_DCS_LONG_WRITE, d, sizeof(d)); \
	} while (0)

/* 0x83: gamma offsets, all neutral (0x80; the 0x00 bytes are the high bits). */
static const u8 ana38401_gamma_neutral[] = {
	0x83,
	0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00, 0x80, 0x80, 0x00,
	0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00, 0x80, 0x80, 0x00,
	0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00, 0x80, 0x80, 0x00,
};

/*
 * AOR and ELVSS per level, dimmest first. These are the rows of the vendor
 * table_normal_br_info (ss_dsi_interpolation_ANA38401_AMSA05RB06.c) with
 * duplicate AOR values dropped, since without the vendor gamma a repeated
 * AOR gives the same luminance. ELVSS is the vendor's high-temperature
 * column, the one it uses above 0 degrees C.
 */
static const struct {
	u16 aor;
	u8 elvss;
} ana38401_levels[] = {
	{ 0x18d, 0x0f }, { 0x18a, 0x0f }, { 0x187, 0x0f }, { 0x184, 0x10 },
	{ 0x180, 0x10 }, { 0x17e, 0x10 }, { 0x17a, 0x10 }, { 0x177, 0x10 },
	{ 0x174, 0x10 }, { 0x16f, 0x11 }, { 0x16d, 0x11 }, { 0x16a, 0x11 },
	{ 0x166, 0x11 }, { 0x163, 0x11 }, { 0x15f, 0x12 }, { 0x15c, 0x12 },
	{ 0x155, 0x12 }, { 0x152, 0x12 }, { 0x14e, 0x13 }, { 0x14a, 0x13 },
	{ 0x144, 0x13 }, { 0x140, 0x13 }, { 0x13a, 0x14 }, { 0x132, 0x14 },
	{ 0x12f, 0x14 }, { 0x128, 0x15 }, { 0x120, 0x15 }, { 0x116, 0x16 },
	{ 0x10e, 0x16 }, { 0x107, 0x16 }, { 0x0fc, 0x17 }, { 0x0f2, 0x17 },
	{ 0x0e6, 0x18 }, { 0x0db, 0x18 }, { 0x0cf, 0x19 }, { 0x0bf, 0x19 },
	{ 0x0b1, 0x19 }, { 0x0a3, 0x19 }, { 0x09c, 0x16 }, { 0x08e, 0x15 },
	{ 0x07c, 0x15 }, { 0x068, 0x15 }, { 0x054, 0x15 }, { 0x03e, 0x14 },
	{ 0x024, 0x14 }, { 0x00e, 0x0f },
};

#define ANA38401_MAX_LEVEL	(ARRAY_SIZE(ana38401_levels) - 1)
/*
 * Boot level, low to spare the AMOLED; systemd-backlight restores the last
 * level once userspace runs.
 */
#define ANA38401_DEFAULT_LEVEL	12

/* Vendor qcom,mdss-dsi-on-command, in order. */
static void ana38401_on_sequence(struct mipi_dsi_multi_context *ctx)
{
	ana38401_short(ctx, 0xb0, 0x7f);
	ana38401_long(ctx, 0xb2, 0x15);
	ana38401_short(ctx, 0xb0, 0xbc);
	ana38401_long(ctx, 0xb2, 0x04);
	ana38401_tx(ctx, MIPI_DSI_DCS_LONG_WRITE, ana38401_gamma_neutral,
		    sizeof(ana38401_gamma_neutral));
	ana38401_long(ctx, 0x90, 0x00, 0x00, 0x0e);	/* AOR */
	ana38401_short(ctx, 0xb0, 0x67);
	ana38401_long(ctx, 0xb2, 0x0f);			/* ELVSS */
	ana38401_short(ctx, 0xb0, 0x4d);
	ana38401_long(ctx, 0xb2, 0x04);			/* ACL off */
	ana38401_short(ctx, 0xb0, 0x49);
	ana38401_long(ctx, 0xb2, 0x99);
	ana38401_short(ctx, 0xb0, 0x36);
	ana38401_long(ctx, 0xb2, 0x10);
	ana38401_short(ctx, 0xb0, 0x35);
	ana38401_long(ctx, 0xb2, 0x01);			/* latch */
}

/*
 * Vendor brightness packet order: AOR (pac_aid_subdivision), ELVSS, then the
 * gamma set, which ends with the 0x35 latch.
 */
static void ana38401_set_level(struct mipi_dsi_multi_context *ctx,
			       unsigned int level)
{
	u16 aor = ana38401_levels[level].aor;
	u8 aid[] = { 0x90, aor >> 8, 0x00, aor & 0xff };
	u8 elvss[] = { 0xb2, ana38401_levels[level].elvss };

	ana38401_tx(ctx, MIPI_DSI_DCS_LONG_WRITE, aid, sizeof(aid));
	ana38401_short(ctx, 0xb0, 0x67);
	ana38401_tx(ctx, MIPI_DSI_DCS_LONG_WRITE, elvss, sizeof(elvss));
	ana38401_tx(ctx, MIPI_DSI_DCS_LONG_WRITE, ana38401_gamma_neutral,
		    sizeof(ana38401_gamma_neutral));
	ana38401_short(ctx, 0xb0, 0x35);
	ana38401_long(ctx, 0xb2, 0x01);
}

/*
 * Bring-up diagnostics: read the DCS status registers on both links and log
 * them. 0x0a power mode (bit 7 booster, 4 sleep out, 2 display on),
 * 0x0b address mode, 0x0c pixel format, 0x0e signal mode (bit 7 TE on),
 * 0x0f self-diagnostic, 0x05 DSI error count, 0xda-0xdc the panel ID the
 * vendor reads. Called with ctx->lock held.
 */
static void ana38401_log_status(struct ana38401 *ctx, const char *when)
{
	static const u8 regs[] = { 0x0a, 0x0b, 0x0c, 0x0e, 0x0f, 0x05,
				   0xda, 0xdb, 0xdc };
	char line[160];
	int i, r, n;

	for (i = 0; i < ARRAY_SIZE(ctx->dsi); i++) {
		n = 0;
		for (r = 0; r < ARRAY_SIZE(regs); r++) {
			u8 val = 0;
			ssize_t ret;

			ret = mipi_dsi_dcs_read(ctx->dsi[i], regs[r], &val, 1);
			if (ret == 1)
				n += scnprintf(line + n, sizeof(line) - n,
					       " %02x=%02x", regs[r], val);
			else
				n += scnprintf(line + n, sizeof(line) - n,
					       " %02x=E%zd", regs[r], ret);
		}
		dev_info(&ctx->dsi[0]->dev, "status %s DSI%d:%s\n", when, i,
			 line);
	}
}

static void ana38401_status_work(struct work_struct *work)
{
	struct ana38401 *ctx = container_of(to_delayed_work(work),
					    struct ana38401, status_work);

	mutex_lock(&ctx->lock);
	if (ctx->on)
		ana38401_log_status(ctx, "after 1 s of frames");
	mutex_unlock(&ctx->lock);
}

/*
 * Bring-up diagnostics: /sys/bus/mipi-dsi/devices/<panel>/dcs.
 *   "<link> r <cmd>"              DCS read of one byte, logged
 *   "<link> w <type> <bytes...>"  one packet of the given DSI type
 * All values are hex. Only while the panel is on.
 */
static ssize_t dcs_store(struct device *dev, struct device_attribute *attr,
			 const char *buf, size_t count)
{
	struct ana38401 *ctx = dev_get_drvdata(dev);
	char *copy, *cur, *tok;
	unsigned int link, val;
	u8 data[64];
	size_t len = 0;
	bool read;
	int ret = -EINVAL;

	copy = kstrndup(buf, count, GFP_KERNEL);
	if (!copy)
		return -ENOMEM;
	cur = strim(copy);

	tok = strsep(&cur, " ");
	if (!tok || kstrtouint(tok, 16, &link) || link >= ARRAY_SIZE(ctx->dsi))
		goto out;
	tok = strsep(&cur, " ");
	if (!tok || (strcmp(tok, "r") && strcmp(tok, "w")))
		goto out;
	read = !strcmp(tok, "r");
	while ((tok = strsep(&cur, " "))) {
		if (!*tok)
			continue;
		if (len >= sizeof(data) || kstrtouint(tok, 16, &val) || val > 0xff)
			goto out;
		data[len++] = val;
	}
	if (read ? len != 1 : len < 2)
		goto out;

	mutex_lock(&ctx->lock);
	if (!ctx->on) {
		ret = -ENODEV;
	} else if (read) {
		/* One byte: msm takes a longer read as a long read. */
		u8 rx[1] = { 0 };
		ssize_t n = mipi_dsi_dcs_read(ctx->dsi[link], data[0], rx,
					      sizeof(rx));

		dev_info(dev, "dcs read DSI%u %02x: %zd: %*ph\n", link, data[0],
			 n, n > 0 ? (int)n : 0, rx);
		ret = n < 0 ? n : 0;
	} else {
		struct mipi_dsi_multi_context dsi_ctx = {
			.dsi = ctx->dsi[link] };

		ana38401_tx(&dsi_ctx, data[0], data + 1, len - 1);
		dev_info(dev, "dcs write DSI%u type %02x: %*ph: %d\n", link,
			 data[0], (int)len - 1, data + 1, dsi_ctx.accum_err);
		ret = dsi_ctx.accum_err;
	}
	mutex_unlock(&ctx->lock);

out:
	kfree(copy);
	return ret ? ret : count;
}
static DEVICE_ATTR_WO(dcs);

static struct attribute *ana38401_attrs[] = {
	&dev_attr_dcs.attr,
	NULL
};
ATTRIBUTE_GROUPS(ana38401);

static int ana38401_wait_tcon_ready(struct ana38401 *ctx)
{
	struct device *dev = &ctx->dsi[0]->dev;
	int val, ret;

	ret = read_poll_timeout(gpiod_get_value_cansleep, val, val != 0,
				1000, ANA38401_TCON_RDY_TIMEOUT_US, false,
				ctx->tcon_rdy_gpio);
	if (val < 0)
		return val;
	if (ret)
		dev_warn(dev, "T-CON not ready after %d ms, continuing\n",
			 ANA38401_TCON_RDY_TIMEOUT_US / 1000);
	else
		dev_info(dev, "T-CON ready after %lld ms\n",
			ktime_ms_delta(ktime_get(), ctx->reset_time));
	return 0;
}

static int ana38401_prepare(struct drm_panel *panel)
{
	struct ana38401 *ctx = to_ana38401(panel);
	struct mipi_dsi_multi_context dsi_ctx = { .dsi = ctx->dsi[0] };
	int ret;

	ret = regulator_enable(ctx->vdd);
	if (ret)
		return ret;
	msleep(ANA38401_VDD_ON_MS);

	/* The DSI links are in LP11 here (prepare_prev_first). */
	gpiod_set_value_cansleep(ctx->reset_gpio, 1);
	msleep(ANA38401_RESET_MS);
	gpiod_set_value_cansleep(ctx->reset_gpio, 0);
	ctx->reset_time = ktime_get();

	ret = ana38401_wait_tcon_ready(ctx);
	if (ret)
		goto err;

	mutex_lock(&ctx->lock);
	ana38401_on_sequence(&dsi_ctx);
	mipi_dsi_dcs_set_tear_on_multi(&dsi_ctx, MIPI_DSI_DCS_TEAR_MODE_VBLANK);
	/* The stored level, even if the backlight is still marked blank. */
	if (ctx->backlight)
		ana38401_set_level(&dsi_ctx, ctx->backlight->props.brightness);
	/* Exit sleep, sent like display on: "11 00". */
	ana38401_tx(&dsi_ctx, MIPI_DSI_DCS_SHORT_WRITE,
		    (const u8[]){ MIPI_DCS_EXIT_SLEEP_MODE, 0x00 }, 2);
	ctx->sleep_out_time = ktime_get();
	ctx->on = !dsi_ctx.accum_err;
	if (ctx->on)
		ana38401_log_status(ctx, "after on-sequence");
	mutex_unlock(&ctx->lock);

	ret = dsi_ctx.accum_err;
	if (ret)
		goto err;

	return 0;

err:
	gpiod_set_value_cansleep(ctx->reset_gpio, 1);
	regulator_disable(ctx->vdd);
	return ret;
}

static int ana38401_enable(struct drm_panel *panel)
{
	struct ana38401 *ctx = to_ana38401(panel);
	struct mipi_dsi_multi_context dsi_ctx = { .dsi = ctx->dsi[0] };
	s64 since_sleep_out = ktime_ms_delta(ktime_get(), ctx->sleep_out_time);

	if (since_sleep_out < ANA38401_DISPLAY_ON_DELAY_MS)
		msleep(ANA38401_DISPLAY_ON_DELAY_MS - since_sleep_out);

	/* display_on_tx_cmds: DCS short write without parameter, "29 00". */
	mutex_lock(&ctx->lock);
	ana38401_tx(&dsi_ctx, MIPI_DSI_DCS_SHORT_WRITE,
		    (const u8[]){ MIPI_DCS_SET_DISPLAY_ON, 0x00 }, 2);
	ana38401_log_status(ctx, "after display on");
	mutex_unlock(&ctx->lock);

	schedule_delayed_work(&ctx->status_work, msecs_to_jiffies(1000));

	return dsi_ctx.accum_err;
}

static int ana38401_disable(struct drm_panel *panel)
{
	struct ana38401 *ctx = to_ana38401(panel);
	struct mipi_dsi_multi_context dsi_ctx = { .dsi = ctx->dsi[0] };

	cancel_delayed_work_sync(&ctx->status_work);

	mutex_lock(&ctx->lock);
	/* display_off_tx_cmds: "28 00", then 100 ms. */
	ana38401_tx(&dsi_ctx, MIPI_DSI_DCS_SHORT_WRITE,
		    (const u8[]){ MIPI_DCS_SET_DISPLAY_OFF, 0x00 }, 2);
	mutex_unlock(&ctx->lock);
	msleep(ANA38401_DISPLAY_OFF_MS);

	return dsi_ctx.accum_err;
}

static int ana38401_unprepare(struct drm_panel *panel)
{
	struct ana38401 *ctx = to_ana38401(panel);

	mutex_lock(&ctx->lock);
	ctx->on = false;
	mutex_unlock(&ctx->lock);

	gpiod_set_value_cansleep(ctx->reset_gpio, 1);
	msleep(ANA38401_VDD_OFF_MS);
	regulator_disable(ctx->vdd);

	return 0;
}

/*
 * The full mode across both links. Per link the vendor timing is 1280x1600,
 * h-front 110, h-pulse 42, h-back 110, v-front 10, v-pulse 2, v-back 6, at
 * 60 Hz and 898 Mbit/s per lane.
 */
static const struct drm_display_mode ana38401_mode = {
	.clock = (2560 + 220 + 84 + 220) * (1600 + 10 + 2 + 6) * 60 / 1000,
	.hdisplay = 2560,
	.hsync_start = 2560 + 220,
	.hsync_end = 2560 + 220 + 84,
	.htotal = 2560 + 220 + 84 + 220,
	.vdisplay = 1600,
	.vsync_start = 1600 + 10,
	.vsync_end = 1600 + 10 + 2,
	.vtotal = 1600 + 10 + 2 + 6,
	.width_mm = 227,
	.height_mm = 142,
	.type = DRM_MODE_TYPE_DRIVER,
};

static int ana38401_get_modes(struct drm_panel *panel,
			      struct drm_connector *connector)
{
	return drm_connector_helper_get_modes_fixed(connector, &ana38401_mode);
}

static const struct drm_panel_funcs ana38401_panel_funcs = {
	.prepare = ana38401_prepare,
	.enable = ana38401_enable,
	.disable = ana38401_disable,
	.unprepare = ana38401_unprepare,
	.get_modes = ana38401_get_modes,
};

static int ana38401_bl_update_status(struct backlight_device *bl)
{
	struct ana38401 *ctx = bl_get_data(bl);
	struct mipi_dsi_multi_context dsi_ctx = { .dsi = ctx->dsi[0] };

	/* Blanking is the panel's display-off; keep the level for later. */
	if (backlight_is_blank(bl))
		return 0;

	mutex_lock(&ctx->lock);
	/* While the panel is off the level is applied by the next prepare. */
	if (ctx->on)
		ana38401_set_level(&dsi_ctx, backlight_get_brightness(bl));
	mutex_unlock(&ctx->lock);

	return dsi_ctx.accum_err;
}

static const struct backlight_ops ana38401_bl_ops = {
	.options = BL_CORE_SUSPENDRESUME,
	.update_status = ana38401_bl_update_status,
};

static int ana38401_probe(struct mipi_dsi_device *dsi)
{
	struct device *dev = &dsi->dev;
	const struct backlight_properties props = {
		.type = BACKLIGHT_RAW,
		.brightness = ANA38401_DEFAULT_LEVEL,
		.max_brightness = ANA38401_MAX_LEVEL,
	};
	struct mipi_dsi_device_info info = { .type = "ANA38401 DSI1" };
	struct mipi_dsi_host *dsi1_host;
	struct device_node *dsi1_node;
	struct ana38401 *ctx;
	int ret, i;

	ctx = devm_drm_panel_alloc(dev, struct ana38401, panel,
				   &ana38401_panel_funcs,
				   DRM_MODE_CONNECTOR_DSI);
	if (IS_ERR(ctx))
		return PTR_ERR(ctx);

	ret = devm_mutex_init(dev, &ctx->lock);
	if (ret)
		return ret;

	ret = devm_delayed_work_autocancel(dev, &ctx->status_work,
					   ana38401_status_work);
	if (ret)
		return ret;

	ctx->vdd = devm_regulator_get(dev, "vdd");
	if (IS_ERR(ctx->vdd))
		return dev_err_probe(dev, PTR_ERR(ctx->vdd),
				     "failed to get vdd supply\n");

	/*
	 * The bootloader left the panel running. Leave reset deasserted until
	 * prepare, so the inherited picture stays until the takeover.
	 */
	ctx->reset_gpio = devm_gpiod_get(dev, "reset", GPIOD_OUT_LOW);
	if (IS_ERR(ctx->reset_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->reset_gpio),
				     "failed to get reset gpio\n");

	ctx->tcon_rdy_gpio = devm_gpiod_get(dev, "tcon-rdy", GPIOD_IN);
	if (IS_ERR(ctx->tcon_rdy_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->tcon_rdy_gpio),
				     "failed to get tcon-rdy gpio\n");

	/* The second link is the panel's port@1, on the other DSI host. */
	dsi1_node = of_graph_get_remote_node(dev->of_node, 1, -1);
	if (!dsi1_node)
		return dev_err_probe(dev, -ENODEV, "no second DSI link in DT\n");

	dsi1_host = of_find_mipi_dsi_host_by_node(dsi1_node);
	info.node = dsi1_node;
	if (!dsi1_host) {
		of_node_put(dsi1_node);
		return dev_err_probe(dev, -EPROBE_DEFER,
				     "second DSI host not registered yet\n");
	}

	ctx->dsi[1] = devm_mipi_dsi_device_register_full(dev, dsi1_host, &info);
	of_node_put(dsi1_node);
	if (IS_ERR(ctx->dsi[1]))
		return dev_err_probe(dev, PTR_ERR(ctx->dsi[1]),
				     "failed to register second DSI device\n");

	ctx->dsi[0] = dsi;
	mipi_dsi_set_drvdata(dsi, ctx);
	mipi_dsi_set_drvdata(ctx->dsi[1], ctx);

	ctx->panel.prepare_prev_first = true;

	ctx->backlight = devm_backlight_device_register(dev, dev_name(dev), dev,
							ctx, &ana38401_bl_ops,
							&props);
	if (IS_ERR(ctx->backlight))
		return dev_err_probe(dev, PTR_ERR(ctx->backlight),
				     "failed to register backlight\n");
	ctx->panel.backlight = ctx->backlight;

	drm_panel_add(&ctx->panel);

	for (i = 0; i < ARRAY_SIZE(ctx->dsi); i++) {
		/*
		 * Command mode (no MIPI_DSI_MODE_VIDEO), continuous HS clock
		 * (qcom,mdss-dsi-force-clock-lane-hs), EoT appended.
		 */
		ctx->dsi[i]->lanes = 4;
		ctx->dsi[i]->format = MIPI_DSI_FMT_RGB888;
		ctx->dsi[i]->mode_flags = 0;

		ret = devm_mipi_dsi_attach(dev, ctx->dsi[i]);
		if (ret < 0) {
			drm_panel_remove(&ctx->panel);
			return dev_err_probe(dev, ret,
					     "failed to attach DSI%d\n", i);
		}
	}

	return 0;
}

static void ana38401_remove(struct mipi_dsi_device *dsi)
{
	struct ana38401 *ctx = mipi_dsi_get_drvdata(dsi);

	drm_panel_remove(&ctx->panel);
}

static const struct of_device_id ana38401_of_match[] = {
	{ .compatible = "samsung,ana38401-amsa05rb06" },
	{ /* sentinel */ }
};
MODULE_DEVICE_TABLE(of, ana38401_of_match);

static struct mipi_dsi_driver ana38401_driver = {
	.probe = ana38401_probe,
	.remove = ana38401_remove,
	.driver = {
		.name = "panel-samsung-ana38401",
		.dev_groups = ana38401_groups,
		.of_match_table = ana38401_of_match,
	},
};
module_mipi_dsi_driver(ana38401_driver);

MODULE_DESCRIPTION("Samsung AMSA05RB06 / Anapass ANA38401 dual-DSI panel");
MODULE_LICENSE("GPL");
