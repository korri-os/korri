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

static struct s2mm005_sample charging(void)
{
	return (struct s2mm005_sample) {
		.state = 21, .flags = 5, .muic_vbus = 1,
		.gauge_status = POWER_SUPPLY_STATUS_CHARGING, .cntl = 5,
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

static int gauge(const char *line)
{
	if (strstr(line, "gauge=\"Charging\""))
		return POWER_SUPPLY_STATUS_CHARGING;
	if (strstr(line, "gauge=\"Full\""))
		return POWER_SUPPLY_STATUS_FULL;
	if (strstr(line, "gauge=\"Not charging\""))
		return POWER_SUPPLY_STATUS_NOT_CHARGING;
	if (strstr(line, "gauge=\"Discharging\""))
		return POWER_SUPPLY_STATUS_DISCHARGING;
	return POWER_SUPPLY_STATUS_UNKNOWN;
}

/* Measured 100 % flap: Charging -> Full -> Charging with VBUS=01, CNTL 5.
 * After qualification, host must hold on every sample (no role change).
 */
static void replay_full_flap(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[512];
	unsigned int count = 0, rows = 0, full = 0;
	struct s2mm005_sample s = { .state = 21, .flags = 5 };

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		if (line[0] == '#')
			continue;
		s.muic_vbus = field(line, "muic_vbus=", 16);
		s.cntl = field(line, "chg_mode=", 16);
		s.gauge_status = gauge(line);
		full += s.gauge_status == POWER_SUPPLY_STATUS_FULL;
		if (!rows) {
			assert(!s2mm005_sink_host(&count, &s));
			assert(!s2mm005_sink_host(&count, &s));
		}
		assert(s2mm005_sink_host(&count, &s));
		rows++;
	}
	assert(rows > 10 && full > 0);
	fclose(f);
}

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
			.gauge_status = gauge(line),
		};
		assert(s2mm005_sink_host(&count, &s) == (qualifies && rows >= 2));
		rows++;
	}
	assert(rows > 0);
	fclose(f);
}

/* The old sourcing traces did not record DFP (or, in the flap trace,
 * CNTL). Give DFP the most permissive value; infer CNTL from the old
 * producer's sourcing=(mode==7). Recorded power/state must still deny host.
 */
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
			.gauge_status = strstr(line, "gauge: \"Charging\"") ? POWER_SUPPLY_STATUS_CHARGING : POWER_SUPPLY_STATUS_DISCHARGING,
		};
		assert(!s2mm005_sink_host(&count, &s));
		rows++;
	}
	assert(rows > 0);
	fclose(f);
}

int main(int argc, char **argv)
{
	unsigned int count = 0;
	struct s2mm005_sample good = charging();

	assert(argc == 6);
	replay(argv[1], false);
	replay(argv[2], true);
	replay_old_sourcing(argv[3]);
	replay_old_sourcing(argv[4]);
	replay_full_flap(argv[5]);
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
	for (unsigned int failure = 0; failure < 7; failure++) {
		struct s2mm005_sample bad = good;

		count = 0;
		qualify(&count, &good);
		switch (failure) {
		case 0: bad.state = 20; break;
		case 1: bad.flags = 4; break;
		case 2: bad.flags = 7; break;
		case 3: bad.flags = 1; break;
		case 4: bad.muic_vbus = 0; break;
		case 5: bad.gauge_status = POWER_SUPPLY_STATUS_DISCHARGING; break;
		case 6: bad.cntl = 7; break;
		}
		assert(!s2mm005_sink_host(&count, &bad));
		assert(count == 0);
		qualify(&count, &good);
		assert(!s2mm005_sink_host(&count, NULL));
		qualify(&count, &good);
		count = 1;
		assert(!s2mm005_sink_host(&count, NULL));
		qualify(&count, &good);
	}
	for (unsigned int vbus = 0; vbus < 256; vbus++) {
		struct s2mm005_sample s = good;
		s.muic_vbus = vbus;
		count = 3;
		assert(s2mm005_sink_host(&count, &s) == (vbus == 1));
	}
	for (unsigned int cntl = 0; cntl < 256; cntl++) {
		struct s2mm005_sample s = good;
		s.cntl = cntl;
		count = 3;
		assert(s2mm005_sink_host(&count, &s) == ((cntl & 7) == 5));
	}
	for (int status = -1; status < 8; status++) {
		struct s2mm005_sample s = good;
		s.gauge_status = status;
		count = 3;
		assert(s2mm005_sink_host(&count, &s) ==
		       (status == POWER_SUPPLY_STATUS_CHARGING ||
			status == POWER_SUPPLY_STATUS_FULL));
	}
	for (unsigned int failure = 0; failure < 7; failure++) {
		struct s2mm005_sample bad = good;

		bad.gauge_status = POWER_SUPPLY_STATUS_FULL;
		count = 3;
		assert(s2mm005_sink_host(&count, &bad));
		switch (failure) {
		case 0: bad.muic_vbus = 0; break;
		case 1: bad.cntl = 7; break;
		case 2: bad.state = 14; break;
		case 3: bad.flags = 4; break;
		case 4: bad.flags = 7; break;
		case 5: bad.flags = 1; break;
		case 6: bad.cntl = 0; break;
		}
		assert(!s2mm005_sink_host(&count, &bad));
		assert(count == 0);
	}
	/* Detached Kishi cannot re-enable, even with a stale Charging gauge. */
	good.state = 14;
	good.flags = 7;
	for (unsigned int tick = 0; tick < 600; tick++)
		assert(!s2mm005_sink_host(&count, &good));
	puts("s2mm005 policy: traces, all flags/states, interlocks, errors, 60s detach passed");
	return 0;
}
