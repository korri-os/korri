// SPDX-License-Identifier: GPL-2.0
#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
/* Values of the kernel's enum power_supply_status (linux/power_supply.h). */
enum power_supply_status {
	POWER_SUPPLY_STATUS_UNKNOWN = 0,
	POWER_SUPPLY_STATUS_CHARGING,
	POWER_SUPPLY_STATUS_DISCHARGING,
	POWER_SUPPLY_STATUS_NOT_CHARGING,
	POWER_SUPPLY_STATUS_FULL,
};
#include "s2mm005-policy.h"

static struct s2mm005_sample sinking_kishi(void)
{
	return (struct s2mm005_sample) {
		.state = 21, .flags = 5, .muic_vbus = 1, .cntl = 5,
		.status1 = 0x21, .gauge_status = POWER_SUPPLY_STATUS_CHARGING,
	};
}

/* Unpowered Kishi on battery, boost off (phase-b-host-transition.txt). */
static struct s2mm005_sample unpowered_kishi(void)
{
	return (struct s2mm005_sample) {
		.state = 14, .flags = 0x88, .muic_vbus = 0, .cntl = 5,
		.status1 = 0x22, .gauge_status = POWER_SUPPLY_STATUS_DISCHARGING,
	};
}

/* Same Kishi on our boost, settled (phase-b: 14, attached 0, VBUS 01). */
static struct s2mm005_sample boosted_kishi(void)
{
	return (struct s2mm005_sample) {
		.state = 14, .flags = 0x88, .muic_vbus = 1, .cntl = 7,
		.status1 = 0x21, .gauge_status = POWER_SUPPLY_STATUS_DISCHARGING,
	};
}

static void qualify(unsigned int *count, const struct s2mm005_sample *s)
{
	assert(!s2mm005_sink_host(count, s));
	assert(!s2mm005_sink_host(count, s));
	assert(s2mm005_sink_host(count, s));
}

static unsigned int field(const char *line, const char *key, int base)
{
	const char *p = strstr(line, key);

	assert(p);
	return strtoul(p + strlen(key), NULL, base);
}

/* --- sink host (charger supplies the port) --------------------------- */

static void replay(const char *path, bool qualifies)
{
	FILE *f = fopen(path, "r");
	char line[4096];
	unsigned int count = 0, rows = 0;

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		struct s2mm005_sample s = {
			.state = field(line, "state=", 10),
			.flags = field(line, "attached=", 10) |
				 (field(line, "source=", 10) << 1) |
				 (field(line, "dfp=", 10) << 2),
			.muic_vbus = field(line, "muic_vbus=", 16),
			.cntl = field(line, "chg_mode=", 16),
		};
		assert(s2mm005_sink_host(&count, &s) == (qualifies && rows >= 2));
		rows++;
	}
	assert(rows > 0);
	fclose(f);
}

static void replay_old_sourcing(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[4096];
	unsigned int count = 0, rows = 0;

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		if (!strstr(line, "sample=Ok(")) {
			assert(!s2mm005_sink_host(&count, NULL));
			continue;
		}
		struct s2mm005_sample s = {
			.state = field(line, "state: ", 10),
			.flags = (strstr(line, "attached: true") ? 1 : 0) |
				 (strstr(line, "source: true") ? 2 : 0) | 4,
			.muic_vbus = field(line, "muic_vbus: ", 10),
			.cntl = strstr(line, "sourcing: true") ? 7 : 5,
		};
		assert(!s2mm005_sink_host(&count, &s));
		rows++;
	}
	assert(rows > 0);
	fclose(f);
}

static int gauge_of(const char *line)
{
	if (strstr(line, "Charging\""))
		return strstr(line, "Discharging\"") ? POWER_SUPPLY_STATUS_DISCHARGING :
		       POWER_SUPPLY_STATUS_CHARGING;
	if (strstr(line, "Full\""))
		return POWER_SUPPLY_STATUS_FULL;
	return POWER_SUPPLY_STATUS_UNKNOWN;
}

/* 100 % with the charger in: the gauge flips while chip facts hold. Host
 * must hold with no role change, and boost must never be requested.
 */
static void replay_full_battery(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[512];
	unsigned int rows = 0, dips = 0;
	struct s2mm005_policy p = { 0 };
	struct s2mm005_sample s = sinking_kishi();
	unsigned long now = 0;

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		struct s2mm005_decision d;

		if (line[0] == '#')
			continue;
		s.muic_vbus = field(line, "muic_vbus=", 16);
		s.cntl = field(line, "chg_mode=", 16);
		s.gauge_status = gauge_of(line);
		dips += s.gauge_status != POWER_SUPPLY_STATUS_CHARGING;
		if (!rows) {
			assert(!s2mm005_decide(&p, &s, now += 100).host);
			assert(!s2mm005_decide(&p, &s, now += 100).host);
		}
		d = s2mm005_decide(&p, &s, now += 100);
		assert(d.host && d.action == S2MM005_NONE);
		rows++;
	}
	assert(rows > 10 && dips > 0);
	fclose(f);
}

/* --- battery source ---------------------------------------------------- */

/* Measured unpowered cycle (boost off, Kishi attached, no charger), then a
 * charger replug. Change events expand to 100 ms polls. Boost must be
 * requested on the first 14 seen twice, and never while VBUS 01.
 */
static void replay_unpowered_cycle(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[256];
	struct s2mm005_sample cur = unpowered_kishi();
	double last = -1, t;
	unsigned int enables = 0, first_enable_state = 0, rows = 0;
	struct s2mm005_policy p = { 0 };

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		if (line[0] == '#')
			continue;
		t = strtod(strchr(line, '=') + 1, NULL);
		if (last >= 0) {
			/* hold the previous facts until this event */
			for (double u = last + 0.1; u < t; u += 0.1) {
				struct s2mm005_decision d =
					s2mm005_decide(&p, &cur, (unsigned long)(u * 1000));
				if (d.action == S2MM005_ENABLE) {
					assert(cur.muic_vbus == 0);
					if (!enables++)
						first_enable_state = cur.state;
					/* the chip is not powered in this replay:
					 * report the enable as failed so the replay
					 * continues as measured (boost stayed off) */
					s2mm005_enable_failed(&p, (unsigned long)(u * 1000));
				}
			}
		}
		cur.state = field(line, "state ", 10);
		cur.flags = field(line, "flags 0x", 16);
		cur.muic_vbus = field(line, "vbus 0x", 16);
		cur.cntl = field(line, "cntl 0x", 16);
		cur.status1 = cur.muic_vbus ? 0x21 : 0x22;
		cur.gauge_status = cur.muic_vbus ? POWER_SUPPLY_STATUS_CHARGING :
				   POWER_SUPPLY_STATUS_DISCHARGING;
		last = t;
		rows++;
	}
	assert(rows > 50 && enables >= 1 && first_enable_state == 14);
	fclose(f);
}

/* Measured boost-on with the Kishi on battery (phase-b-host-transition.txt):
 * three source samples, our boost on, state 0 for ~0.7 s, then 14 with
 * attached 0. Boost must hold through the window and after it, until the
 * old daemon's deadline turned the charger back to mode 5 (cntl revoke).
 */
static void replay_battery_attach(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[4096];
	struct s2mm005_policy p = { 0 };
	unsigned int rows = 0;
	bool enabled = false, revoked = false, requested = false;
	unsigned long held = 0;

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		unsigned long now = field(line, "elapsed_ms=", 10) + 10000;
		struct s2mm005_sample s = {
			.state = field(line, "state: ", 10),
			.flags = (strstr(line, "attached: true") ? 1 : 0) |
				 (strstr(line, "source: true") ? 2 : 0),
			.muic_vbus = field(line, "muic_vbus: ", 10),
			.cntl = field(line, "cntl: ", 10),
			.status1 = field(line, "charger: ", 10),
			.gauge_status = POWER_SUPPLY_STATUS_DISCHARGING,
		};
		struct s2mm005_decision d;

		rows++;
		if (!enabled && s.cntl == 7) {
			/* the trace's boost was verified host_ms before this row */
			held = now - field(line, "host_ms=Some(", 10);
			assert(requested);
			s2mm005_enabled(&p, held);
			enabled = true;
		}
		d = s2mm005_decide(&p, &s, now);
		if (!enabled) {
			if (d.action == S2MM005_ENABLE) {
				assert(s.cntl == 5 && s.muic_vbus == 0);
				requested = true;
			}
			continue;
		}
		if (s.cntl == 7) {
			assert(d.action == S2MM005_NONE && d.host);
		} else if (!revoked) {
			assert(d.action == S2MM005_DISABLE);
			assert(!strcmp(d.why, "charger left OTG mode"));
			assert(now - held > S2MM005_GRACE_MS);
			s2mm005_disabled(&p);
			revoked = true;
		} else {
			/* fixed retry delay: the Kishi in 14 cannot re-enable at once */
			assert(d.action == S2MM005_NONE);
		}
	}
	assert(rows == 51 && enabled && revoked);
	fclose(f);
}

static void start(struct s2mm005_policy *p, unsigned long *now)
{
	struct s2mm005_sample s = unpowered_kishi();

	assert(s2mm005_decide(p, &s, *now += 100).action == S2MM005_NONE);
	assert(s2mm005_decide(p, &s, *now += 100).action == S2MM005_ENABLE);
	s2mm005_enabled(p, *now);
}

static void settle(struct s2mm005_policy *p, unsigned long *now)
{
	struct s2mm005_sample s = boosted_kishi();

	s.state = 0;
	for (int i = 0; i < 35; i++)	/* 0.7 s of state 0 at 20 ms */
		assert(s2mm005_decide(p, &s, *now += 20).action == S2MM005_NONE);
	s.state = 14;
	while (p->boost == S2MM005_BOOST_GRACE)
		assert(s2mm005_decide(p, &s, *now += 20).host);
	assert(p->boost == S2MM005_BOOST_ON);
}

static void charger_insertion_while_boosting(void)
{
	/* 17 / 20 / 21 with a VBUS dip, in and after the window. */
	for (int in_grace = 0; in_grace < 2; in_grace++) {
		unsigned char states[] = { 17, 20, 21, 18, 29, 52 };

		for (unsigned int i = 0; i < sizeof(states); i++) {
			struct s2mm005_policy p = { 0 };
			unsigned long now = 0;
			struct s2mm005_sample s = boosted_kishi();
			struct s2mm005_decision d;

			start(&p, &now);
			if (!in_grace)
				settle(&p, &now);
			s.state = states[i];
			s.flags = 0x99;
			d = s2mm005_decide(&p, &s, now += 20);
			assert(d.action == S2MM005_DISABLE && !d.host);
			s2mm005_disabled(&p);
			/* then the charger: sink host requalifies, no boost */
			s = sinking_kishi();
			assert(!s2mm005_decide(&p, &s, now += 20).host);
			assert(!s2mm005_decide(&p, &s, now += 20).host);
			d = s2mm005_decide(&p, &s, now += 20);
			assert(d.host && d.action == S2MM005_NONE);
			for (int k = 0; k < 500; k++)
				assert(s2mm005_decide(&p, &s, now += 100).action ==
				       S2MM005_NONE);
		}
	}
}

static void detach_and_window(void)
{
	struct s2mm005_policy p = { 0 };
	unsigned long now = 0, t0;
	struct s2mm005_sample s = boosted_kishi();
	struct s2mm005_decision d;

	/* after the window, state 0 revokes at once, fixed retry delay */
	start(&p, &now);
	settle(&p, &now);
	s.state = 0;
	d = s2mm005_decide(&p, &s, now += 20);
	assert(d.action == S2MM005_DISABLE && !strcmp(d.why, "detach"));
	s2mm005_disabled(&p);
	t0 = now;
	s = unpowered_kishi();
	while (now - t0 < S2MM005_RETRY_MS)
		assert(s2mm005_decide(&p, &s, now += 100).action == S2MM005_NONE);
	assert(s2mm005_decide(&p, &s, now += 100).action == S2MM005_ENABLE);

	/* a window that ends in state 0 is a failed start: 3, 6 ... 60 s */
	memset(&p, 0, sizeof(p));
	now = 0;
	for (unsigned long expect = 3000; expect <= 120000; expect *= 2) {
		unsigned long wait = expect > S2MM005_MAX_BACKOFF_MS ?
				     S2MM005_MAX_BACKOFF_MS : expect;

		start(&p, &now);
		t0 = now;
		s = boosted_kishi();
		s.state = 0;
		while (now + 20 - t0 < S2MM005_GRACE_MS)
			assert(s2mm005_decide(&p, &s, now += 20).action == S2MM005_NONE);
		/* the window never extends: the next state-0 poll revokes */
		d = s2mm005_decide(&p, &s, now += 20);
		assert(d.action == S2MM005_DISABLE);
		s2mm005_disabled(&p);
		t0 = now;
		s = unpowered_kishi();
		while (now + 100 - t0 < wait)
			assert(s2mm005_decide(&p, &s, now += 100).action == S2MM005_NONE);
		now = t0 + wait;
	}
	/* 60 s of stable boost resets the backoff */
	start(&p, &now);
	settle(&p, &now);
	s = boosted_kishi();
	t0 = p.since_ms;
	while (now - t0 < S2MM005_STABLE_MS)
		assert(s2mm005_decide(&p, &s, now += 1000).host);
	assert(p.backoff_ms == 0);
}

static void every_revoke_path(void)
{
	for (int in_grace = 0; in_grace < 2; in_grace++) {
		for (int failure = 0; failure < 12; failure++) {
			struct s2mm005_policy p = { 0 };
			unsigned long now = 0;
			struct s2mm005_sample s = boosted_kishi();
			struct s2mm005_decision d;
			const struct s2mm005_sample *in = &s;

			start(&p, &now);
			if (!in_grace)
				settle(&p, &now);
			switch (failure) {
			case 0: in = NULL; break;			/* read error */
			case 1: s.cntl = 5; break;			/* forced CHG_ON */
			case 2: s.cntl = 0; break;			/* suspend */
			case 3: s.status3 = S2MM005_ST3_OTGFAIL; break;
			case 4: s.status1 |= S2MM005_ST1_VBUSOVP; break;
			case 5: s.status1 |= S2MM005_ST1_VBUSLIMIT; break;
			case 6: s.status1 |= S2MM005_ST1_WPCINPOK; break;
			case 7: s.devt[0] = 0x40; break;		/* dedicated charger */
			case 8: s.devt[0] = 0x04; break;		/* USB (PC) */
			case 9: s.devt[2] = 0x80; break;		/* AFC TA */
			case 10: s.state = 42; break;			/* unknown */
			case 11: s.state = in_grace ? 6 : 15; break;	/* not 0/14 in window */
			}
			d = s2mm005_decide(&p, in, now += 20);
			assert(d.action == S2MM005_DISABLE && !d.host && d.why);
			assert(p.boost == S2MM005_BOOST_OFF);
			/* failed revoke: retry, then stop with boost off */
			s2mm005_disable_failed(&p);
			s = boosted_kishi();
			for (int k = 0; k < 10; k++) {
				d = s2mm005_decide(&p, &s, now += 20);
				assert(d.action == S2MM005_DISABLE && !d.host);
			}
			s2mm005_disabled(&p);
			assert(p.boost == S2MM005_BOOST_STOPPED);
			s = unpowered_kishi();
			for (int k = 0; k < 1000; k++) {
				d = s2mm005_decide(&p, &s, now += 100);
				assert(d.action == S2MM005_NONE && !d.host);
			}
			s = sinking_kishi();
			for (int k = 0; k < 10; k++)
				assert(!s2mm005_decide(&p, &s, now += 100).host);
		}
	}
}

static void enable_interlocks(void)
{
	struct s2mm005_sample good = unpowered_kishi();

	assert(s2mm005_may_source(&good));
	for (unsigned int st = 0; st < 256; st++) {
		struct s2mm005_sample s = good;

		s.state = st;
		assert(s2mm005_may_source(&s) ==
		       (st == 1 || (st >= 3 && st <= 7) || st == 14));
	}
	for (unsigned int v = 0; v < 256; v++) {
		struct s2mm005_sample s = good;

		s.status1 = v;
		assert(s2mm005_may_source(&s) == (v == 0x02 || v == 0x22));
		s = good;
		s.muic_vbus = v;
		assert(s2mm005_may_source(&s) == (v == 0));
		s = good;
		s.cntl = v;
		assert(s2mm005_may_source(&s) == ((v & 7) == 5));
		s = good;
		s.status3 = v;
		assert(s2mm005_may_source(&s) == !(v & S2MM005_ST3_OTGFAIL));
		s = good;
		s.devt[0] = v;
		assert(s2mm005_may_source(&s) == (v == 0 || v == 0x08 || v == 0x80));
		s = good;
		s.devt[1] = v;
		assert(s2mm005_may_source(&s) == (v == 0));
		s = good;
		s.devt[2] = v;
		assert(s2mm005_may_source(&s) == (v == 0));
	}
	for (int g = -1; g < 8; g++) {
		struct s2mm005_sample s = good;

		s.gauge_status = g;
		assert(s2mm005_may_source(&s) == (g == POWER_SUPPLY_STATUS_DISCHARGING));
	}
	/* Two fresh polls; any bad sample or read error resets. */
	{
		struct s2mm005_policy p = { 0 };
		struct s2mm005_sample bad = good;
		unsigned long now = 0;

		bad.muic_vbus = 1;
		assert(s2mm005_decide(&p, &good, now += 100).action == S2MM005_NONE);
		assert(s2mm005_decide(&p, &bad, now += 100).action == S2MM005_NONE);
		assert(s2mm005_decide(&p, &good, now += 100).action == S2MM005_NONE);
		assert(s2mm005_decide(&p, NULL, now += 100).action == S2MM005_NONE);
		assert(s2mm005_decide(&p, &good, now += 100).action == S2MM005_NONE);
		assert(s2mm005_decide(&p, &good, now += 100).action == S2MM005_ENABLE);
		/* a failed enable backs off */
		s2mm005_enable_failed(&p, now);
		for (int k = 0; k < 29; k++)
			assert(s2mm005_decide(&p, &good, now += 100).action == S2MM005_NONE);
	}
}

int main(int argc, char **argv)
{
	unsigned int count = 0;
	struct s2mm005_sample good = sinking_kishi();

	assert(argc == 8);
	replay(argv[1], false);
	replay(argv[2], true);
	replay_old_sourcing(argv[3]);
	replay_old_sourcing(argv[4]);
	replay_full_battery(argv[5]);
	replay_full_battery(argv[6]);
	replay_unpowered_cycle(argv[7]);
	replay_battery_attach(argv[4]);
	for (unsigned int state = 0; state < 256; state++) {
		for (unsigned int flags = 0; flags < 256; flags++) {
			struct s2mm005_sample s = good;

			s.state = state;
			s.flags = flags;
			count = 3;
			assert(s2mm005_sink_host(&count, &s) ==
			       (state == 21 && (flags & 7) == 5));
		}
	}
	for (unsigned int failure = 0; failure < 8; failure++) {
		struct s2mm005_sample bad = good;

		count = 0;
		qualify(&count, &good);
		switch (failure) {
		case 0: bad.state = 20; break;
		case 1: bad.flags = 4; break;
		case 2: bad.flags = 7; break;
		case 3: bad.flags = 1; break;
		case 4: bad.muic_vbus = 0; break;
		case 5: bad.cntl = 7; break;
		case 6: bad.cntl = 0; break;
		case 7: bad.state = 14; break;
		}
		assert(!s2mm005_sink_host(&count, &bad));
		assert(count == 0);
		qualify(&count, &good);
		assert(!s2mm005_sink_host(&count, NULL));
		qualify(&count, &good);
	}
	enable_interlocks();
	charger_insertion_while_boosting();
	detach_and_window();
	every_revoke_path();
	puts("s2mm005 policy: sink host, battery source, every revoke path, traces passed");
	return 0;
}
