// SPDX-License-Identifier: GPL-2.0
#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "s2mm005-policy.h"

static struct s2mm005_sample sinking_kishi(void)
{
	return (struct s2mm005_sample) {
		.state = 21, .flags = 5, .muic_vbus = 1, .cntl = 5,
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

/* Phase A observer format (phase-a-transition.txt, sink-host-charging.txt).
 * The first two rows of a qualifying trace wait; every later row is host.
 */
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

/* The old sourcing traces (phase-b-*.txt) did not record DFP or, in the flap
 * trace, CNTL. Give DFP the most permissive value; infer CNTL from the old
 * producer's sourcing=(mode==7). The recorded chip state must still deny.
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
		};
		assert(!s2mm005_sink_host(&count, &s));
		rows++;
	}
	assert(rows > 0);
	fclose(f);
}

/* Measured at 100 % with the charger in: the gauge flips Charging, Full and
 * Discharging while VBUS=01 and CNTL 0x05 hold. FUNC_STATE was not recorded;
 * the kernel showed no PD change, so state 21/attached/DFP is used. After
 * qualification, host must hold on every sample (no role change).
 */
static void replay_full_battery(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[512];
	unsigned int count = 0, rows = 0, dips = 0;
	struct s2mm005_sample s = sinking_kishi();

	assert(f);
	while (fgets(line, sizeof(line), f)) {
		if (line[0] == '#')
			continue;
		s.muic_vbus = field(line, "muic_vbus=", 16);
		s.cntl = field(line, "chg_mode=", 16);
		dips += !strstr(line, "gauge=\"Charging\"");
		if (!rows) {
			assert(!s2mm005_sink_host(&count, &s));
			assert(!s2mm005_sink_host(&count, &s));
		}
		assert(s2mm005_sink_host(&count, &s));
		rows++;
	}
	assert(rows > 10 && dips > 0);
	fclose(f);
}

int main(int argc, char **argv)
{
	unsigned int count = 0;
	struct s2mm005_sample good = sinking_kishi();

	assert(argc == 7);
	replay(argv[1], false);
	replay(argv[2], true);
	replay_old_sourcing(argv[3]);
	replay_old_sourcing(argv[4]);
	replay_full_battery(argv[5]);
	replay_full_battery(argv[6]);
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
	/* Each interlock and a read error revoke at once and requalify. */
	for (unsigned int failure = 0; failure < 8; failure++) {
		struct s2mm005_sample bad = good;

		count = 0;
		qualify(&count, &good);
		switch (failure) {
		case 0: bad.state = 20; break;
		case 1: bad.flags = 4; break;	/* detached */
		case 2: bad.flags = 7; break;	/* source=1 */
		case 3: bad.flags = 1; break;	/* DFP=0 */
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
	/* Detached Kishi (source-ready, VBUS 00) never enables for 60 s. */
	good.state = 14;
	good.flags = 7;
	good.muic_vbus = 0;
	for (unsigned int tick = 0; tick < 600; tick++)
		assert(!s2mm005_sink_host(&count, &good));
	puts("s2mm005 policy: 6 traces, all flags/states/VBUS/CNTL, interlocks, errors, 60s detach passed");
	return 0;
}
