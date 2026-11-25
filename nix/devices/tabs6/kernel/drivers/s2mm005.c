// SPDX-License-Identifier: GPL-2.0
/* Galaxy Tab S6 USB-C policy: sink-host data role, and a battery-powered
 * source for non-PD sinks (the Kishi) the way Samsung's kernel does it.
 *
 * The s2mm005 stays strictly read-only: register-address write plus
 * repeated-START read, no mailbox commands, no interrupt clear, no firmware.
 * SM5705 writes are limited to the stock boost registers (CNTL mode bits,
 * CHGCNTL6 OTG current, FLEDCNTL6 boost voltage), read-modify-write of the
 * bits Samsung's sm5705_charger_oper.c touches, each verified by readback.
 * Grounding: usb-role/typec-kernel.md, Samsung kernel 37cf30da.
 */
#include <linux/delay.h>
#include <linux/i2c.h>
#include <linux/jiffies.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/power_supply.h>
#include <linux/ratelimit.h>
#include <linux/slab.h>
#include <linux/string.h>
#include <linux/usb/role.h>
#include <linux/usb/typec.h>
#include <linux/workqueue.h>

#include "s2mm005-policy.h"

#define S2MM005_POLL_MS		100
#define S2MM005_BOOST_POLL_MS	20	/* while boost may be on */
/* dwc3 reports the new role only after its drd work tears down the old one
 * (about 200 ms for host to device). Treat a longer mismatch as an outside
 * write to the role file.
 */
#define S2MM005_MISMATCH_MS	1000

#define SM5705_CHG_ADDR		0x49
#define SM5705_MUIC_ADDR	0x25
#define SM5705_STATUS1		0x08	/* STATUS1..STATUS4, then CNTL at 0x0c */
#define SM5705_CNTL		0x0c
#define SM5705_CHGCNTL6		0x14
#define SM5705_FLEDCNTL6	0x20
#define SM5705_MODE_MASK	0x07
#define SM5705_MODE_SUSPEND	0
#define SM5705_OTGCUR_MASK	0x0c
#define SM5705_OTGCUR_1500MA	(3 << 2)	/* stock USB_OTG table */
#define SM5705_OTGCUR_500MA	(0 << 2)	/* stock CHG_ON table */
#define SM5705_BSTOUT_MASK	0x0f
#define SM5705_BSTOUT_5100MV	0x0b
#define SM5705_BSTOUT_4500MV	0x05
#define SM5705_OFF_TRIES	3
#define SM5705_MUIC_INT1	0x03	/* INT1..INT3, read-to-clear latches */

/* Diagnostic only: while boosting, log every poll's STATUS1-4, MUIC INT1-3
 * (reading clears them; nothing else in Linux uses them) and gauge current,
 * to find a charger-arrival discriminator that works while we source VBUS.
 */
static bool boost_telemetry;
module_param(boost_telemetry, bool, 0644);
MODULE_PARM_DESC(boost_telemetry, "log charger/MUIC/gauge telemetry every poll while boosting");

struct s2mm005 {
	struct i2c_client *client;
	struct i2c_adapter *sm5705;
	struct power_supply *gauge;
	struct usb_role_switch *role_sw;
	struct typec_port *port;
	struct typec_partner *partner;
	struct delayed_work poll;
	struct s2mm005_policy policy;
	unsigned long mismatch_since;
	bool mismatch;
	bool first_poll_logged;
	struct s2mm005_sample last;
	bool last_valid;
	enum usb_role requested;
	struct ratelimit_state facts_rs;
	struct ratelimit_state telemetry_rs;
};

static unsigned long s2mm005_now(void)
{
	return (unsigned long)jiffies64_to_msecs(get_jiffies_64());
}

/* Register address followed by a repeated-START read. */
static int s2mm005_read(struct i2c_adapter *adap, u16 addr,
		       u8 *reg, u16 reg_len, u8 *out, u16 len)
{
	struct i2c_msg msgs[] = {
		{ .addr = addr, .len = reg_len, .buf = reg },
		{ .addr = addr, .flags = I2C_M_RD, .len = len, .buf = out },
	};
	int ret = i2c_transfer(adap, msgs, ARRAY_SIZE(msgs));

	return ret == ARRAY_SIZE(msgs) ? 0 : (ret < 0 ? ret : -EIO);
}

/* The only write path, SM5705 charger only, stock boost bits only. */
static int sm5705_update(struct s2mm005 *s, u8 reg, u8 mask, u8 val)
{
	u8 old, now, buf[2];
	struct i2c_msg msg = { .addr = SM5705_CHG_ADDR, .len = 2, .buf = buf };
	int ret;

	if (!((reg == SM5705_CNTL && mask == SM5705_MODE_MASK) ||
	      (reg == SM5705_CHGCNTL6 && mask == SM5705_OTGCUR_MASK) ||
	      (reg == SM5705_FLEDCNTL6 && mask == SM5705_BSTOUT_MASK)))
		return -EPERM;
	ret = s2mm005_read(s->sm5705, SM5705_CHG_ADDR, &reg, 1, &old, 1);
	if (ret)
		return ret;
	buf[0] = reg;
	buf[1] = (old & ~mask) | (val & mask);
	if (buf[1] != old) {
		ret = i2c_transfer(s->sm5705, &msg, 1);
		if (ret != 1)
			return ret < 0 ? ret : -EIO;
	}
	ret = s2mm005_read(s->sm5705, SM5705_CHG_ADDR, &reg, 1, &now, 1);
	if (ret)
		return ret;
	return (now & mask) == (val & mask) ? 0 : -EIO;
}

static int s2mm005_sample(struct s2mm005 *s, struct s2mm005_sample *sample,
			  bool with_gauge)
{
	u8 dummy_reg[] = { 0x00, 0x08 }, func_reg[] = { 0x00, 0x20 };
	u8 vbus_reg = 0x15, devt_reg = 0x0a, chg_reg = SM5705_STATUS1;
	u8 dummy, func[4], chg[5];
	union power_supply_propval status = { .intval = POWER_SUPPLY_STATUS_UNKNOWN };
	int ret;

	memset(sample, 0, sizeof(*sample));
	/* Match the observer's ordinary dummy read, not a flash/boot command. */
	ret = s2mm005_read(s->client->adapter, s->client->addr,
			   dummy_reg, sizeof(dummy_reg), &dummy, 1);
	if (ret)
		return ret;
	ret = s2mm005_read(s->client->adapter, s->client->addr,
			   func_reg, sizeof(func_reg), func, sizeof(func));
	if (ret)
		return ret;
	sample->state = func[0];
	sample->flags = func[3];
	ret = s2mm005_read(s->sm5705, SM5705_MUIC_ADDR, &vbus_reg, 1,
			   &sample->muic_vbus, 1);
	if (ret)
		return ret;
	ret = s2mm005_read(s->sm5705, SM5705_MUIC_ADDR, &devt_reg, 1,
			   sample->devt, sizeof(sample->devt));
	if (ret)
		return ret;
	/* STATUS1..4 are status, not read-to-clear interrupt latches (INT1-4
	 * at 0x00-0x03 are never read). CNTL follows at 0x0c.
	 */
	ret = s2mm005_read(s->sm5705, SM5705_CHG_ADDR, &chg_reg, 1, chg, sizeof(chg));
	if (ret)
		return ret;
	sample->status1 = chg[0];
	sample->status3 = chg[2];
	sample->cntl = chg[4];
	/* The gauge decides only enables; it cannot see our own boost. */
	if (with_gauge) {
		ret = power_supply_get_property(s->gauge, POWER_SUPPLY_PROP_STATUS,
						&status);
		if (ret)
			return ret;
	}
	sample->gauge_status = status.intval;
	return 0;
}

/* Boost on in the existing helper's order: configuration, a fresh interlock
 * re-check, then CNTL mode 7, each verified.
 */
static int s2mm005_boost_on(struct s2mm005 *s)
{
	struct s2mm005_sample fresh;
	int ret;

	ret = sm5705_update(s, SM5705_FLEDCNTL6, SM5705_BSTOUT_MASK, SM5705_BSTOUT_5100MV);
	if (ret)
		return ret;
	ret = sm5705_update(s, SM5705_CHGCNTL6, SM5705_OTGCUR_MASK, SM5705_OTGCUR_1500MA);
	if (ret)
		return ret;
	/* Configuration is not permission. */
	ret = s2mm005_sample(s, &fresh, true);
	if (ret)
		return ret;
	if (!s2mm005_may_source(&fresh))
		return -EAGAIN;
	return sm5705_update(s, SM5705_CNTL, SM5705_MODE_MASK, S2MM005_MODE_USB_OTG);
}

/* Boost off: every 7 -> 5 transition passes through SUSPEND (mode 0) for
 * 100 ms (stock does it only for gauge id < 5; we keep it always). Returns
 * 0 only when CNTL reads mode 5.
 */
static int s2mm005_boost_off(struct s2mm005 *s)
{
	u8 reg = SM5705_CNTL, cntl;
	int ret = -EIO, i;

	for (i = 0; i < SM5705_OFF_TRIES; i++) {
		ret = s2mm005_read(s->sm5705, SM5705_CHG_ADDR, &reg, 1, &cntl, 1);
		if (!ret && (cntl & SM5705_MODE_MASK) != S2MM005_MODE_CHG_ON) {
			ret = sm5705_update(s, SM5705_CNTL, SM5705_MODE_MASK,
					    SM5705_MODE_SUSPEND);
			if (!ret)
				msleep(100);
		}
		if (!ret)
			ret = sm5705_update(s, SM5705_CNTL, SM5705_MODE_MASK,
					    S2MM005_MODE_CHG_ON);
		if (!ret)
			break;
	}
	if (ret)
		return ret;
	/* Back to the stock CHG_ON table; failures here do not source VBUS. */
	if (sm5705_update(s, SM5705_FLEDCNTL6, SM5705_BSTOUT_MASK, SM5705_BSTOUT_4500MV) ||
	    sm5705_update(s, SM5705_CHGCNTL6, SM5705_OTGCUR_MASK, SM5705_OTGCUR_500MA))
		dev_warn(&s->client->dev, "boost settings not restored\n");
	return 0;
}

static int s2mm005_role(struct s2mm005 *s, enum usb_role role)
{
	int ret;

	if (s->requested == role) {
		if (usb_role_switch_get_role(s->role_sw) == role) {
			s->mismatch = false;
			return 0;
		}
		/* The role file stays writable (dwc3 allows user space control).
		 * Undo an outside write instead of trusting the cached decision.
		 */
		if (!s->mismatch) {
			s->mismatch = true;
			s->mismatch_since = s2mm005_now();
		}
		if (s2mm005_now() - s->mismatch_since < S2MM005_MISMATCH_MS)
			return 0;
		dev_warn_ratelimited(&s->client->dev, "role changed outside policy; %s again\n",
				     usb_role_string(role));
	}
	s->mismatch = false;
	ret = usb_role_switch_set_role(s->role_sw, role);
	if (ret) {
		dev_err_ratelimited(&s->client->dev, "role %s failed: %d\n",
				   usb_role_string(role), ret);
		return ret;
	}
	if (s->requested == role)
		return 0;
	s->requested = role;
	typec_set_data_role(s->port, role == USB_ROLE_HOST ? TYPEC_HOST : TYPEC_DEVICE);
	dev_info(&s->client->dev, "data role %s\n", usb_role_string(role));
	return 0;
}

static void s2mm005_partner(struct s2mm005 *s, const struct s2mm005_sample *sample)
{
	struct typec_partner_desc desc = { .accessory = TYPEC_ACCESSORY_NONE };
	bool boosting = s2mm005_boosting(&s->policy);

	if (!sample || (!(sample->flags & 1) && !boosting)) {
		if (s->partner) {
			typec_unregister_partner(s->partner);
			s->partner = NULL;
		}
		return;
	}
	/* Our boost is the only way this port sources VBUS. */
	typec_set_pwr_role(s->port, boosting ? TYPEC_SOURCE : TYPEC_SINK);
	if (!s->partner) {
		s->partner = typec_register_partner(s->port, &desc);
		if (IS_ERR(s->partner)) {
			dev_err_ratelimited(&s->client->dev, "partner registration: %ld\n",
					   PTR_ERR(s->partner));
			s->partner = NULL;
		}
	}
}

static void s2mm005_log_facts(struct s2mm005 *s, const struct s2mm005_sample *x)
{
	if (s->last_valid && !memcmp(&s->last, x, sizeof(*x)))
		return;
	s->last = *x;
	s->last_valid = true;
	if (__ratelimit(&s->facts_rs))
		dev_info(&s->client->dev,
			 "facts: state %u flags 0x%02x vbus 0x%02x cntl 0x%02x st1 0x%02x st3 0x%02x devt %02x%02x%02x gauge %d (sink %u/3 src %u/%u boost %d)\n",
			 x->state, x->flags, x->muic_vbus, x->cntl, x->status1,
			 x->status3, x->devt[0], x->devt[1], x->devt[2],
			 x->gauge_status, s->policy.sink_polls,
			 s->policy.source_polls, S2MM005_SOURCE_POLLS,
			 s->policy.boost);
}

static void s2mm005_telemetry(struct s2mm005 *s)
{
	u8 chg_reg = SM5705_STATUS1, int_reg = SM5705_MUIC_INT1, chg[5], mint[3];
	union power_supply_propval cur = { 0 };

	if (s2mm005_read(s->sm5705, SM5705_CHG_ADDR, &chg_reg, 1, chg, sizeof(chg)) ||
	    s2mm005_read(s->sm5705, SM5705_MUIC_ADDR, &int_reg, 1, mint, sizeof(mint)) ||
	    power_supply_get_property(s->gauge, POWER_SUPPLY_PROP_CURRENT_NOW, &cur)) {
		if (__ratelimit(&s->telemetry_rs))
			dev_info(&s->client->dev, "telemetry: read error\n");
		return;
	}
	if (__ratelimit(&s->telemetry_rs))
		dev_info(&s->client->dev,
			 "telemetry: st1-4 %02x %02x %02x %02x cntl %02x muic-int %02x %02x %02x i_ua %d\n",
			 chg[0], chg[1], chg[2], chg[3], chg[4], mint[0], mint[1],
			 mint[2], cur.intval);
}

static void s2mm005_poll(struct work_struct *work)
{
	struct s2mm005 *s = container_of(to_delayed_work(work), struct s2mm005, poll);
	struct s2mm005_decision d;
	struct s2mm005_sample sample;
	bool boosting = s2mm005_boosting(&s->policy);
	bool was_host = s->requested == USB_ROLE_HOST;
	int ret;

	if (!s->first_poll_logged)
		dev_info(&s->client->dev, "first poll: reading\n");
	ret = s2mm005_sample(s, &sample, !boosting);
	if (ret)
		dev_err_ratelimited(&s->client->dev, "status read failed: %d\n", ret);
	else
		s2mm005_log_facts(s, &sample);
	if (!s->first_poll_logged) {
		s->first_poll_logged = true;
		dev_info(&s->client->dev, "first poll: %s\n", ret ? "read error" : "ok");
	}
	if (ret)
		s->last_valid = false;

	d = s2mm005_decide(&s->policy, ret ? NULL : &sample, s2mm005_now());
	switch (d.action) {
	case S2MM005_ENABLE:
		ret = s2mm005_boost_on(s);
		if (!ret) {
			s2mm005_enabled(&s->policy, s2mm005_now());
			dev_info(&s->client->dev, "boost on (%s): mode 7 verified\n", d.why);
			d.host = true;
		} else {
			dev_warn(&s->client->dev, "boost on failed: %d; off\n", ret);
			s2mm005_enable_failed(&s->policy, s2mm005_now());
			if (s2mm005_boost_off(s))
				s2mm005_disable_failed(&s->policy);
		}
		break;
	case S2MM005_DISABLE:
		/* Boost off with readback first, then the data role. */
		if (s->policy.boost != S2MM005_BOOST_FAULT)
			dev_info(&s->client->dev, "boost off: %s\n", d.why);
		if (!s2mm005_boost_off(s)) {
			s2mm005_disabled(&s->policy);
			dev_info(&s->client->dev, "boost off: mode 5 verified%s\n",
				 s->policy.boost == S2MM005_BOOST_STOPPED ?
				 "; policy stopped until rebind" : "");
		} else {
			if (s->policy.boost != S2MM005_BOOST_FAULT)
				dev_crit(&s->client->dev, "boost off NOT verified; retrying\n");
			s2mm005_disable_failed(&s->policy);
		}
		d.host = false;
		break;
	case S2MM005_NONE:
		break;
	}
	if (boost_telemetry && s2mm005_boosting(&s->policy))
		s2mm005_telemetry(s);
	if (d.host && !was_host && !s2mm005_boosting(&s->policy))
		dev_info(&s->client->dev, "qualified 3/3\n");

	if (s2mm005_role(s, d.host ? USB_ROLE_HOST : USB_ROLE_DEVICE)) {
		/* A failed set may have had partial effects. Force device. */
		s->requested = USB_ROLE_NONE;
		s2mm005_role(s, USB_ROLE_DEVICE);
	}
	s2mm005_partner(s, ret ? NULL : &sample);
	/* IRQ123 can remain asserted without acknowledgement. Never request it:
	 * polling remains live without clearing/masking any controller register.
	 */
	schedule_delayed_work(&s->poll, msecs_to_jiffies(
		s2mm005_boosting(&s->policy) || s->policy.boost == S2MM005_BOOST_FAULT ?
		S2MM005_BOOST_POLL_MS : S2MM005_POLL_MS));
}

static int s2mm005_probe(struct i2c_client *client)
{
	struct typec_capability cap = {
		.type = TYPEC_PORT_DRP,
		.data = TYPEC_PORT_DRD,
		.prefer_role = TYPEC_NO_PREFERRED_ROLE,
		.no_mode_control = true,
	};
	struct device *dev = &client->dev;
	struct device_node *node;
	struct s2mm005 *s;
	int ret;

	dev_info(dev, "probe start\n");
	if (!i2c_check_functionality(client->adapter, I2C_FUNC_I2C))
		return -EOPNOTSUPP;
	s = devm_kzalloc(dev, sizeof(*s), GFP_KERNEL);
	if (!s)
		return -ENOMEM;
	s->client = client;
	i2c_set_clientdata(client, s);
	ratelimit_state_init(&s->facts_rs, HZ, 50);
	ratelimit_state_init(&s->telemetry_rs, HZ, 60);
	INIT_DELAYED_WORK(&s->poll, s2mm005_poll);
	s->role_sw = usb_role_switch_get(dev);
	if (IS_ERR(s->role_sw))
		return dev_err_probe(dev, PTR_ERR(s->role_sw), "USB role switch\n");
	if (!s->role_sw)
		return dev_err_probe(dev, -ENODEV, "missing USB role switch connection\n");
	/* Fail closed: device before any fact is read, so every later probe
	 * failure leaves the gadget lifeline selected.
	 */
	ret = usb_role_switch_set_role(s->role_sw, USB_ROLE_DEVICE);
	if (ret)
		goto put_role;

	node = of_parse_phandle(dev->of_node, "samsung,sm5705-i2c", 0);
	if (!node) {
		ret = -EINVAL;
		goto put_role;
	}
	s->sm5705 = of_get_i2c_adapter_by_node(node);
	of_node_put(node);
	if (!s->sm5705) {
		ret = -EPROBE_DEFER;
		goto put_role;
	}
	if (!i2c_check_functionality(s->sm5705, I2C_FUNC_I2C)) {
		ret = -EOPNOTSUPP;
		goto put_adapter;
	}
	s->gauge = power_supply_get_by_reference(dev_fwnode(dev), "power-supplies");
	if (IS_ERR_OR_NULL(s->gauge)) {
		ret = s->gauge ? PTR_ERR(s->gauge) : -EPROBE_DEFER;
		goto put_adapter;
	}
	/* Never inherit a boost: boot-vbus should already have turned it off. */
	ret = s2mm005_boost_off(s);
	if (ret)
		goto put_gauge;
	cap.fwnode = dev_fwnode(dev);
	s->port = typec_register_port(dev, &cap);
	if (IS_ERR(s->port)) {
		ret = PTR_ERR(s->port);
		goto put_gauge;
	}
	ret = s2mm005_role(s, USB_ROLE_DEVICE);
	if (ret)
		goto unregister_port;
	dev_info(dev, "policy: sink host and battery source, %d/%d ms polling, IRQ unused\n",
		 S2MM005_POLL_MS, S2MM005_BOOST_POLL_MS);
	schedule_delayed_work(&s->poll, 0);
	return 0;

unregister_port:
	typec_unregister_port(s->port);
put_gauge:
	power_supply_put(s->gauge);
put_adapter:
	i2c_put_adapter(s->sm5705);
put_role:
	usb_role_switch_put(s->role_sw);
	return dev_err_probe(dev, ret, "policy probe\n");
}

/* Stop sampling, boost off with readback, then device. */
static int s2mm005_quiesce(struct s2mm005 *s)
{
	int ret;

	cancel_delayed_work_sync(&s->poll);
	ret = s2mm005_boost_off(s);
	if (ret)
		dev_crit(&s->client->dev, "boost off NOT verified at stop: %d\n", ret);
	s->policy = (struct s2mm005_policy){ 0 };
	return s2mm005_role(s, USB_ROLE_DEVICE) ?: ret;
}

static void s2mm005_shutdown(struct i2c_client *client)
{
	s2mm005_quiesce(i2c_get_clientdata(client));
}

static void s2mm005_remove(struct i2c_client *client)
{
	struct s2mm005 *s = i2c_get_clientdata(client);

	s2mm005_quiesce(s);
	s2mm005_partner(s, NULL);
	typec_unregister_port(s->port);
	power_supply_put(s->gauge);
	i2c_put_adapter(s->sm5705);
	usb_role_switch_put(s->role_sw);
}

static int s2mm005_suspend(struct device *dev)
{
	struct s2mm005 *s = dev_get_drvdata(dev);
	int ret = s2mm005_quiesce(s);

	if (ret)
		schedule_delayed_work(&s->poll, 0);
	return ret;
}

static int s2mm005_resume(struct device *dev)
{
	struct s2mm005 *s = dev_get_drvdata(dev);

	schedule_delayed_work(&s->poll, 0);
	return 0;
}

static DEFINE_SIMPLE_DEV_PM_OPS(s2mm005_pm, s2mm005_suspend, s2mm005_resume);

static const struct of_device_id s2mm005_of_match[] = {
	{ .compatible = "samsung,s2mm005-read-only" },
	{ }
};
MODULE_DEVICE_TABLE(of, s2mm005_of_match);

static struct i2c_driver s2mm005_driver = {
	.driver = {
		.name = "s2mm005-read-only",
		.of_match_table = s2mm005_of_match,
		.pm = pm_sleep_ptr(&s2mm005_pm),
	},
	.probe = s2mm005_probe,
	.remove = s2mm005_remove,
	.shutdown = s2mm005_shutdown,
};
module_i2c_driver(s2mm005_driver);

MODULE_DESCRIPTION("Galaxy Tab S6 USB-C data role and battery source policy");
MODULE_LICENSE("GPL");
