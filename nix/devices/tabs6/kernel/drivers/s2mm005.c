// SPDX-License-Identifier: GPL-2.0
/* Galaxy Tab S6 read-only s2mm005 sink-host data-role policy.
 * Register grounding: usb-role/typec.md, Samsung kernel 37cf30da.
 * No chip initialization, interrupt acknowledgement, PD commands or VBUS writes.
 */
#include <linux/i2c.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/power_supply.h>
#include <linux/slab.h>
#include <linux/usb/role.h>
#include <linux/usb/typec.h>
#include <linux/workqueue.h>

#include "s2mm005-policy.h"

#define S2MM005_POLL_MS 100

struct s2mm005 {
	struct i2c_client *client;
	struct i2c_adapter *sm5705;
	struct power_supply *gauge;
	struct usb_role_switch *role_sw;
	struct typec_port *port;
	struct typec_partner *partner;
	struct delayed_work poll;
	unsigned int consecutive;
	enum usb_role requested;
};

/* The only bus operation: register address followed by repeated-START read.
 * SM5705 addresses remain unclaimed so existing charge maintenance and the
 * boot boost-off service retain their unchanged, non-forced i2c-dev access.
 */
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

static int s2mm005_sample(struct s2mm005 *s, struct s2mm005_sample *sample)
{
	u8 dummy_reg[] = { 0x00, 0x08 }, func_reg[] = { 0x00, 0x20 };
	u8 muic_reg = 0x15, cntl_reg = 0x0c, dummy, func[4];
	union power_supply_propval status;
	int ret;

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
	ret = s2mm005_read(s->sm5705, 0x25, &muic_reg, 1,
			   &sample->muic_vbus, 1);
	if (ret)
		return ret;
	ret = s2mm005_read(s->sm5705, 0x49, &cntl_reg, 1, &sample->cntl, 1);
	if (ret)
		return ret;
	/* sm5705_fg_get_property(STATUS) performs fresh CURRENT/SOC reads;
	 * its five-second change-notification poll is NOT a status cache.
	 */
	ret = power_supply_get_property(s->gauge, POWER_SUPPLY_PROP_STATUS, &status);
	if (ret)
		return ret;
	sample->gauge_status = status.intval;
	return 0;
}

static int s2mm005_role(struct s2mm005 *s, enum usb_role role)
{
	int ret;

	if (s->requested == role)
		return 0;
	ret = usb_role_switch_set_role(s->role_sw, role);
	if (ret) {
		dev_err_ratelimited(&s->client->dev, "role %s failed: %d\n",
				   usb_role_string(role), ret);
		return ret;
	}
	s->requested = role;
	typec_set_data_role(s->port, role == USB_ROLE_HOST ? TYPEC_HOST : TYPEC_DEVICE);
	dev_info(&s->client->dev, "data role %s (read-only, no VBUS writes)\n",
		 usb_role_string(role));
	return 0;
}

static void s2mm005_partner(struct s2mm005 *s, const struct s2mm005_sample *sample)
{
	struct typec_partner_desc desc = { .accessory = TYPEC_ACCESSORY_NONE };

	if (!sample || !(sample->flags & 1)) {
		if (s->partner) {
			typec_unregister_partner(s->partner);
			s->partner = NULL;
		}
		return;
	}
	/* Report observed power direction, never command it. Do not infer a PD
	 * contract, cable orientation or USB revision from CC numeric values.
	 */
	typec_set_pwr_role(s->port, sample->flags & 2 ? TYPEC_SOURCE : TYPEC_SINK);
	if (!s->partner) {
		s->partner = typec_register_partner(s->port, &desc);
		if (IS_ERR(s->partner)) {
			dev_err_ratelimited(&s->client->dev, "partner registration: %ld\n",
					   PTR_ERR(s->partner));
			s->partner = NULL;
		}
	}
}

static void s2mm005_poll(struct work_struct *work)
{
	struct s2mm005 *s = container_of(to_delayed_work(work), struct s2mm005, poll);
	struct s2mm005_sample sample;
	int ret = s2mm005_sample(s, &sample);
	bool host = s2mm005_sink_host(&s->consecutive, ret ? NULL : &sample);

	if (ret)
		dev_err_ratelimited(&s->client->dev, "status read failed: %d; device\n", ret);
	if (s2mm005_role(s, host ? USB_ROLE_HOST : USB_ROLE_DEVICE)) {
		s->consecutive = 0;
		/* A failed set may have had partial effects. Force device again. */
		s->requested = USB_ROLE_NONE;
		s2mm005_role(s, USB_ROLE_DEVICE);
	}
	s2mm005_partner(s, ret ? NULL : &sample);
	/* IRQ123 can remain asserted without acknowledgement. Never request it:
	 * polling remains live without clearing/masking any controller register.
	 */
	schedule_delayed_work(&s->poll, msecs_to_jiffies(S2MM005_POLL_MS));
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

	if (!i2c_check_functionality(client->adapter, I2C_FUNC_I2C))
		return -EOPNOTSUPP;
	s = devm_kzalloc(dev, sizeof(*s), GFP_KERNEL);
	if (!s)
		return -ENOMEM;
	s->client = client;
	i2c_set_clientdata(client, s);
	INIT_DELAYED_WORK(&s->poll, s2mm005_poll);
	s->role_sw = usb_role_switch_get(dev);
	if (IS_ERR(s->role_sw))
		return dev_err_probe(dev, PTR_ERR(s->role_sw), "USB role switch\n");
	if (!s->role_sw)
		return dev_err_probe(dev, -ENODEV, "missing USB role switch connection\n");

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
	cap.fwnode = dev_fwnode(dev);
	s->port = typec_register_port(dev, &cap);
	if (IS_ERR(s->port)) {
		ret = PTR_ERR(s->port);
		goto put_gauge;
	}
	ret = s2mm005_role(s, USB_ROLE_DEVICE);
	if (ret)
		goto unregister_port;
	dev_info(dev, "read-only sink-host policy, %d ms polling, IRQ unused\n",
		 S2MM005_POLL_MS);
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
	return dev_err_probe(dev, ret, "read-only policy probe\n");
}

static int s2mm005_quiesce(struct s2mm005 *s)
{
	cancel_delayed_work_sync(&s->poll);
	s->consecutive = 0;
	return s2mm005_role(s, USB_ROLE_DEVICE);
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

MODULE_DESCRIPTION("Galaxy Tab S6 read-only s2mm005 USB data-role policy");
MODULE_LICENSE("GPL");
