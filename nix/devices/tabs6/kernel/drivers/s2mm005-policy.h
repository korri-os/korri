/* SPDX-License-Identifier: GPL-2.0 */
#ifndef S2MM005_POLICY_H
#define S2MM005_POLICY_H

/* Pure Tab S6 USB-C policy, shared by the kernel driver and the host test.
 * Grounding: Samsung 37cf30da. s2mm005.h function_status_t, s2mm005_cc.c
 * process_cc_attach, sm5705.h STATUS1/STATUS3, muic_regmap_sm5705.c DEVT1-3.
 * See usb-role/typec-kernel.md. The caller supplies bool and the enum
 * power_supply_status values (linux headers, or the host test).
 *
 * Two independent paths:
 *  - sink host: data role only, while the charger supplies the port;
 *  - battery source: boost on (SM5705 mode 7) for a non-PD sink such as the
 *    unpowered Kishi, which the chip reports in source states.
 */
struct s2mm005_sample {
	unsigned char state;		/* FUNC_STATE byte 0 (PD state) */
	unsigned char flags;		/* byte 3: attached b0, source b1, DFP b2 */
	unsigned char muic_vbus;	/* MUIC RSVDID1 0x15 */
	unsigned char cntl;		/* SM5705 CNTL 0x0c, mode in bits 2:0 */
	unsigned char status1;		/* SM5705 STATUS1 0x08 */
	unsigned char status3;		/* SM5705 STATUS3 0x0a */
	unsigned char devt[3];		/* MUIC DEVT1-3 0x0a-0x0c */
	int gauge_status;		/* POWER_SUPPLY_STATUS_* */
};

#define S2MM005_MODE_CHG_ON	5
#define S2MM005_MODE_USB_OTG	7
#define S2MM005_ST1_VBUSPOK	0x01
#define S2MM005_ST1_VBUSUVLO	0x02
#define S2MM005_ST1_VBUSOVP	0x04
#define S2MM005_ST1_VBUSLIMIT	0x08
#define S2MM005_ST1_WPCINPOK	0x10
#define S2MM005_ST1_WPCINUVLO	0x20
#define S2MM005_ST3_OTGFAIL	0x04

#define S2MM005_SOURCE_POLLS	2
#define S2MM005_GRACE_MS	1500
#define S2MM005_REDETECT_MS	2500
#define S2MM005_RETRY_MS	3000
#define S2MM005_MAX_BACKOFF_MS	60000
#define S2MM005_STABLE_MS	60000

/* Sink host: three consecutive fresh samples of SNK_Ready, attached,
 * not source, DFP, MUIC VBUS 01 and charger mode 5. NULL is a read error.
 * Only enables wait; revocations clear qualification on the same call.
 */
static inline bool s2mm005_sink_host(unsigned int *consecutive,
				   const struct s2mm005_sample *s)
{
	if (!s || s->state != 21 || (s->flags & 7) != 5 ||
	    s->muic_vbus != 1 || (s->cntl & 7) != S2MM005_MODE_CHG_ON) {
		*consecutive = 0;
		return false;
	}
	if (*consecutive < 3)
		++*consecutive;
	return *consecutive == 3;
}

/* Stock turns boost on in SRC states 1, 3-7 (process_cc_attach). State 14,
 * SRC_Wait_New_Capabilities, is our measured extension: the unpowered,
 * non-PD Kishi sits there (0 <-> 14 every ~15.8 s) and stock keeps boost on
 * in 14.
 */
static inline bool s2mm005_source_state(unsigned char state)
{
	return state == 1 || (state >= 3 && state <= 7) || state == 14;
}

/* DEVT3 bit 2, DCD_OUT_SDP (data contact detect timed out), stays latched
 * after the charger is unplugged (measured: DEVT3 0x04 with VBUS 00, STATUS1
 * 0x22, Discharging). It is a stale classification, not incoming power.
 */
#define S2MM005_DEVT3_DCD_OUT_SDP	0x04

/* MUIC: none, passive UART (measured with the unpowered Kishi) or OTG.
 * Every charger, PC or unknown classification denies.
 */
static inline bool s2mm005_muic_quiet(const struct s2mm005_sample *s)
{
	return (s->devt[0] == 0 || s->devt[0] == 0x08 || s->devt[0] == 0x80) &&
	       s->devt[1] == 0 && (s->devt[2] & ~S2MM005_DEVT3_DCD_OUT_SDP) == 0;
}

/* A FUNC_STATE read with every flag bit set (attached, source, DFP, both Rp
 * bits, both short bits, reset at once) is not a plausible state: measured
 * once as state 15 / flags 0xff between two 14s while boosting (2227.98 s).
 */
static inline bool s2mm005_suspect(const struct s2mm005_sample *s)
{
	return s->flags == 0xff;
}

static inline bool s2mm005_faults(const struct s2mm005_sample *s)
{
	return (s->status3 & S2MM005_ST3_OTGFAIL) ||
	       (s->status1 & (S2MM005_ST1_VBUSOVP | S2MM005_ST1_VBUSLIMIT |
			      S2MM005_ST1_WPCINPOK));
}

/* Enable interlock, boost off: a source state, no incoming power (MUIC VBUS
 * 00, STATUS1 exactly VBUSUVLO with no VBUSPOK/fault/wireless input, gauge
 * Discharging, quiet MUIC), no faults, CNTL mode 5.
 */
static inline bool s2mm005_may_source(const struct s2mm005_sample *s)
{
	return s2mm005_source_state(s->state) && s->muic_vbus == 0 &&
	       (s->status1 & ~S2MM005_ST1_WPCINUVLO) == S2MM005_ST1_VBUSUVLO &&
	       s->gauge_status == POWER_SUPPLY_STATUS_DISCHARGING &&
	       s2mm005_muic_quiet(s) && !s2mm005_faults(s) &&
	       (s->cntl & 7) == S2MM005_MODE_CHG_ON;
}

enum s2mm005_boost {
	S2MM005_BOOST_OFF,
	S2MM005_BOOST_GRACE,	/* verified on, within the fixed window */
	S2MM005_BOOST_ON,
	S2MM005_BOOST_FAULT,	/* boost-off unverified: keep retrying, no enables */
	S2MM005_BOOST_STOPPED,	/* off verified after a fault: device until rebind */
};

enum s2mm005_action {
	S2MM005_NONE,
	S2MM005_ENABLE,
	S2MM005_DISABLE,
};

struct s2mm005_policy {
	enum s2mm005_boost boost;
	unsigned int sink_polls;
	unsigned int source_polls;
	unsigned long since_ms;
	unsigned long retry_at_ms;
	unsigned long backoff_ms;
	bool suspect;			/* last FUNC_STATE read was suspect */
	bool zero;			/* in a state-0 episode while boosting */
	unsigned long zero_since_ms;
};

struct s2mm005_decision {
	enum s2mm005_action action;
	bool host;
	const char *why;
};

static inline bool s2mm005_boosting(const struct s2mm005_policy *p)
{
	return p->boost == S2MM005_BOOST_GRACE || p->boost == S2MM005_BOOST_ON;
}

static inline void s2mm005_backoff(struct s2mm005_policy *p, unsigned long now)
{
	p->backoff_ms = p->backoff_ms ? 2 * p->backoff_ms : S2MM005_RETRY_MS;
	if (p->backoff_ms > S2MM005_MAX_BACKOFF_MS)
		p->backoff_ms = S2MM005_MAX_BACKOFF_MS;
	p->retry_at_ms = now + p->backoff_ms;
}

/* Why boost must go off now, or NULL. Applies every poll while boosting.
 * Updates the state-0 episode.
 */
static inline const char *s2mm005_revoke(struct s2mm005_policy *p,
				       const struct s2mm005_sample *s,
				       unsigned long now)
{
	bool grace = p->boost == S2MM005_BOOST_GRACE &&
		     now - p->since_ms < S2MM005_GRACE_MS;

	if (!s)
		return "read error";
	if ((s->cntl & 7) != S2MM005_MODE_USB_OTG)
		return "charger left OTG mode";
	if (s2mm005_faults(s))
		return "fault";
	if (!s2mm005_muic_quiet(s))
		return "MUIC charger/PC signature";
	/* Re-read a suspect FUNC_STATE on the next poll; a second suspect read
	 * in a row is a read error. Every check above stays immediate.
	 */
	if (s2mm005_suspect(s)) {
		if (p->suspect)
			return "read error";
		p->suspect = true;
		return NULL;
	}
	p->suspect = false;
	/* The chip re-detects even while we boost: state 0 for ~0.7 s after a
	 * verified enable (phase-b-host-transition.txt) and again on its ~16 s
	 * cycle (kishi-battery-boost-cycle.txt; measured 1.35-1.39 s with boost
	 * on, owner decision 2.5 s). A state-0 episode is tolerated
	 * for at most S2MM005_REDETECT_MS from its first poll, never extended;
	 * a source state ends it. Longer is a detach.
	 */
	if (s->state == 0) {
		if (!p->zero) {
			p->zero = true;
			p->zero_since_ms = now;
		}
		return now - p->zero_since_ms < S2MM005_REDETECT_MS ? NULL : "detach";
	}
	p->zero = false;
	/* In the fixed window only 14 (besides 0) is tolerated. */
	if (grace)
		return s->state == 14 ? NULL : "state in grace";
	return s2mm005_source_state(s->state) ? NULL : "left source states";
}

/* One poll. s is NULL on any read error; now is monotonic milliseconds.
 * The caller performs the action, then reports success or failure.
 */
static inline struct s2mm005_decision
s2mm005_decide(struct s2mm005_policy *p, const struct s2mm005_sample *s,
	       unsigned long now)
{
	struct s2mm005_decision d = { S2MM005_NONE, false, NULL };

	if (p->boost == S2MM005_BOOST_STOPPED) {
		d.why = "stopped after a boost-off failure";
		return d;
	}
	if (p->boost == S2MM005_BOOST_FAULT) {
		d.action = S2MM005_DISABLE;
		d.why = "boost-off unverified";
		return d;
	}
	if (s2mm005_boosting(p)) {
		const char *why = s2mm005_revoke(p, s, now);
		bool grace = p->boost == S2MM005_BOOST_GRACE;

		p->sink_polls = 0;
		p->source_polls = 0;
		if (why) {
			/* A start that fails in or at the end of its window backs
			 * off; any other revoke waits the fixed retry delay.
			 */
			if (grace)
				s2mm005_backoff(p, now);
			else
				p->retry_at_ms = now + S2MM005_RETRY_MS;
			p->boost = S2MM005_BOOST_OFF;
			d.action = S2MM005_DISABLE;
			d.why = why;
			return d;
		}
		/* A start succeeds only once the window is over and the chip shows a
		 * source state; a window ending in state 0 stays a start, so its
		 * revoke backs off.
		 */
		if (grace && now - p->since_ms >= S2MM005_GRACE_MS && !p->zero)
			p->boost = S2MM005_BOOST_ON;
		if (p->boost == S2MM005_BOOST_ON &&
		    now - p->since_ms >= S2MM005_STABLE_MS)
			p->backoff_ms = 0;
		d.host = true;
		return d;
	}
	if (s2mm005_sink_host(&p->sink_polls, s)) {
		p->source_polls = 0;
		d.host = true;
		return d;
	}
	if (s && (long)(now - p->retry_at_ms) >= 0 && !s2mm005_suspect(s) &&
	    s2mm005_may_source(s)) {
		if (++p->source_polls >= S2MM005_SOURCE_POLLS) {
			p->source_polls = 0;
			d.action = S2MM005_ENABLE;
			d.why = "battery source";
		}
	} else {
		p->source_polls = 0;
	}
	return d;
}

/* Boost verified on by readback: start the fixed window. */
static inline void s2mm005_enabled(struct s2mm005_policy *p, unsigned long now)
{
	p->boost = S2MM005_BOOST_GRACE;
	p->since_ms = now;
	p->zero = false;
	p->suspect = false;
}

/* Enable failed part way: the caller turns boost off; this start failed. */
static inline void s2mm005_enable_failed(struct s2mm005_policy *p,
					unsigned long now)
{
	p->boost = S2MM005_BOOST_OFF;
	s2mm005_backoff(p, now);
}

/* Boost-off could not be verified after retries: no further enables. */
static inline void s2mm005_disable_failed(struct s2mm005_policy *p)
{
	p->boost = S2MM005_BOOST_FAULT;
}

/* Boost-off verified. After a fault the policy stays stopped (device role,
 * no enables) until the driver is rebound.
 */
static inline void s2mm005_disabled(struct s2mm005_policy *p)
{
	p->boost = p->boost == S2MM005_BOOST_FAULT ? S2MM005_BOOST_STOPPED :
						      S2MM005_BOOST_OFF;
}
#endif
