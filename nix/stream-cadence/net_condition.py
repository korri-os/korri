"""The conditions of a real network on a private pair's own loopback (`--net`).

A same-device private pair streams over the loopback of its own network
namespace (`fixture_namespace.PairNamespace`). `--net` gives that loopback
loss, burst loss, reordering and delay with tc-netem(8), in netem's own
words. Nothing is applied anywhere else: every command names `dev lo`, runs
in a thread that is in the pair's namespace and not in the product's, and
the pair's namespace has no other interface.

netem alone would drop whole frames, not packets. Sunshine sends the packets
of a frame's FEC block in one `sendmsg` with UDP_SEGMENT (`send_batch` in
`src/platform/linux/misc.cpp`). A qdisc sees that batch as one buffer, and
segmentation happens only after the qdisc. A token bucket (tbf) as the root
splits every buffer larger than its burst into its datagrams before they
reach netem, its one child (`tbf_segment` in the kernel's `sch_tbf.c`). Its
burst is one Ethernet frame at an MTU of 1500, so the loopback takes that
MTU too: a TCP buffer of the loopback's own MTU of 65536 has segments that
no burst of one frame passes. The bucket's rate is far above what a stream
sends, so it shapes nothing.

The conditions apply to every packet on the loopback, in both directions:
a delay of D gives a round trip of 2 D, and loss falls on video, audio,
control, RTSP and the pairing's HTTPS alike.
"""

import json
import os
import re
import socket
import subprocess

DEVICE = "lo"
# Ethernet's MTU, and the largest frame that the token bucket passes whole:
# the MTU and Ethernet's header of 14 bytes, which the loopback carries.
MTU = 1500
FRAME = MTU + 14
# Far above a stream's bitrate: the bucket splits GSO batches and shapes nothing.
RATE = "100gbit"
# The bucket's own queue in bytes. netem replaces that queue as its child.
BUCKET_LIMIT = 100000
ROOT, CHILD = "1:", "10:"
CLONE_NEWNET = 0x40000000
# tc's own errors are kept, up to this many characters.
WORDS_KEPT = 2000

NUMBER = r"(\d+(?:\.\d+)?)"
LIMIT_MS = 1000.0


def number(value: float) -> str:
    """A number as the record and tc get it: 30 for 30.0, 0.5 for 0.5."""
    text = repr(float(value))
    return text[:-2] if text.endswith(".0") else text


def percent(word: str, what: str) -> float:
    match = re.fullmatch(NUMBER + "%", word)
    if not match:
        raise ValueError(f"--net: {what} is a percent, such as 1% or 0.5%; {word!r} is not.")
    value = float(match.group(1))
    if not 0 < value < 100:
        raise ValueError(f"--net: {what} lies between 0 and 100 percent, both left out; {word} does not.")
    return value


def condition(spec: str) -> dict:
    """The words of `--net`, checked, in one order; a ValueError says what is wrong.

    netem's own words, a part of them: `delay TIME` in milliseconds,
    `loss random PERCENT`, `loss gemodel P R` (Gilbert's model: P is the
    chance to enter the state of loss, R the chance to leave it, and every
    packet in that state is lost) and `reorder PERCENT`, which needs a delay
    (netem sends the reordered packets at once and delays the others).
    Jitter, correlation and the other words of netem are not taken.
    """
    words = spec.split()
    if not words:
        raise ValueError("--net names no condition. Give netem's words, such as 'loss random 1%' or 'delay 30ms'.")
    delay: float | None = None
    loss: tuple[str, list[float]] | None = None
    reorder: float | None = None
    seen: set[str] = set()
    index = 0
    while index < len(words):
        word = words[index]
        index += 1
        if word in seen:
            raise ValueError(f"--net: {word} is given more than once; netem takes it once.")
        seen.add(word)
        if word == "delay":
            given = words[index] if index < len(words) else ""
            index += 1
            match = re.fullmatch(NUMBER + "ms", given)
            if not match:
                raise ValueError(f"--net: delay takes a time in milliseconds, such as 30ms; {given!r} is not.")
            delay = float(match.group(1))
            if not 0 < delay <= LIMIT_MS:
                raise ValueError(f"--net: a delay lies above 0 and at most {LIMIT_MS:g} ms; {given} does not.")
        elif word == "loss":
            model = words[index] if index < len(words) else ""
            index += 1
            if model == "random":
                given = words[index] if index < len(words) else ""
                index += 1
                loss = ("random", [percent(given, "loss random")])
            elif model == "gemodel":
                given_pr = words[index : index + 2]
                if len(given_pr) < 2 or not all(re.fullmatch(NUMBER + "%", value) for value in given_pr):
                    raise ValueError(
                        "--net: loss gemodel takes two percents: p, the chance to enter the state of "
                        "loss, and r, the chance to leave it, such as 'loss gemodel 1.3158% 25%'."
                    )
                index += 2
                loss = ("gemodel", [percent(given_pr[0], "gemodel p"), percent(given_pr[1], "gemodel r")])
            else:
                raise ValueError(
                    "--net: loss takes netem's model: 'loss random PERCENT' or 'loss gemodel P R'."
                )
        elif word == "reorder":
            given = words[index] if index < len(words) else ""
            index += 1
            reorder = percent(given, "reorder")
        else:
            raise ValueError(
                f"--net: netem's word {word!r} is not one that the tool takes; it takes delay, loss, reorder."
            )
    if reorder is not None and delay is None:
        raise ValueError(
            "--net: reorder needs a delay. netem sends the reordered packets at once and delays the others."
        )
    parts = []
    if delay is not None:
        parts.append(f"delay {number(delay)}ms")
    mean_loss, burst = 0.0, None
    if loss is not None:
        model, values = loss
        parts.append(f"loss {model} " + " ".join(number(value) + "%" for value in values))
        if model == "random":
            mean_loss = values[0]
        else:
            p, r = values
            mean_loss, burst = 100 * p / (p + r), 100 / r
    if reorder is not None:
        parts.append(f"reorder {number(reorder)}%")
    return {
        "netem": " ".join(parts),
        "delay_ms": delay or 0.0,
        "mean_loss_percent": mean_loss,
        "mean_burst_packets": burst,
        "reorder_percent": reorder or 0.0,
    }


def commands(netem: str, iproute2: str) -> list[list[str]]:
    """The three commands that give the loopback its condition, in their order."""
    ip, tc = iproute2 + "/bin/ip", iproute2 + "/bin/tc"
    return [
        [ip, "link", "set", "dev", DEVICE, "mtu", str(MTU)],
        [tc, "qdisc", "add", "dev", DEVICE, "root", "handle", ROOT, "tbf",
         "rate", RATE, "burst", str(FRAME), "limit", str(BUCKET_LIMIT)],
        [tc, "qdisc", "add", "dev", DEVICE, "parent", ROOT + "1", "handle", CHILD, "netem", *netem.split()],
    ]


def read(command: list[str], enter: int | None = None) -> str:
    """A read-only command's output; with `enter`, in that network namespace."""
    def into() -> None:
        if enter is not None:
            os.setns(enter, CLONE_NEWNET)

    done = subprocess.run(
        command, capture_output=True, text=True, timeout=20, preexec_fn=into if enter is not None else None
    )
    if done.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {done.stderr.strip()[:WORDS_KEPT]}")
    return done.stdout


def snapshot(iproute2: str, enter: int | None = None) -> dict:
    """The qdiscs and the MTU of every interface of a network namespace. It only reads."""
    qdiscs = json.loads(read([iproute2 + "/bin/tc", "-j", "qdisc", "show"], enter) or "[]")
    links = json.loads(read([iproute2 + "/bin/ip", "-j", "link", "show"], enter) or "[]")
    return {
        "namespace": os.readlink(f"/proc/self/fd/{enter}") if enter is not None else os.readlink("/proc/thread-self/ns/net"),
        "mtu": {row["ifname"]: row["mtu"] for row in links},
        "qdiscs": [
            {"dev": row.get("dev"), "kind": row["kind"], "handle": row.get("handle"), "parent": row.get("parent")}
            for row in qdiscs
        ],
    }


def untouched(before: dict, after: dict) -> list[str]:
    """What changed in the product's network that this tool could have changed; empty when nothing.

    A netem or tbf qdisc that was not there before, and an MTU that changed on
    an interface that was there before. An interface that appears, as a
    container's, is no change of the product's network.
    """
    changes = [
        f"{name}: MTU {before['mtu'][name]} became {after['mtu'][name]}"
        for name in sorted(before["mtu"])
        if name in after["mtu"] and after["mtu"][name] != before["mtu"][name]
    ]
    known = {json.dumps(row, sort_keys=True) for row in before["qdiscs"]}
    changes += [
        f"a {row['kind']} qdisc appeared on {row['dev']} ({row['handle']}, parent {row['parent']})"
        for row in after["qdiscs"]
        if row["kind"] in ("netem", "tbf") and json.dumps(row, sort_keys=True) not in known
    ]
    return changes


def apply(pair, netem: str, iproute2: str) -> dict:
    """Give the pair's loopback its condition, and prove where it went.

    `pair` is the agent's `PairNamespace`. Its `inside` refuses a thread that
    is in no pair namespace or in the product's: then nothing is run. The
    product's network is read first, from its own namespace, as evidence.
    """
    namespace = pair.inside()
    product = pair.original["net"]
    before = snapshot(iproute2, enter=product)
    interfaces = sorted(name for _, name in socket.if_nameindex())
    if interfaces != [DEVICE]:
        raise RuntimeError(f"The pair's network namespace has {', '.join(interfaces)}, not only its own loopback. No condition is applied.")
    run = commands(netem, iproute2)
    for command in run:
        done = subprocess.run(command, capture_output=True, text=True, timeout=20)
        if done.returncode:
            raise RuntimeError(f"{' '.join(command)} failed: {done.stderr.strip()[:WORDS_KEPT]}")
    here = snapshot(iproute2)
    try:
        init = os.readlink("/proc/1/ns/net")
    except OSError:
        init = None
    return {
        "commands": run,
        "namespace": namespace,
        "product_namespace": before["namespace"],
        # PID 1's, where it can be read: the namespace of the product's services.
        "init_namespace": init,
        "interfaces": interfaces,
        "mtu": here["mtu"][DEVICE],
        "qdiscs": [row for row in here["qdiscs"] if row["dev"] == DEVICE],
        "product_before": before,
    }


def product_network(pair, iproute2: str) -> dict:
    """The product's network now, read in its own namespace."""
    return snapshot(iproute2, enter=pair.original["net"])


def counters(iproute2: str) -> dict:
    """What netem on this namespace's loopback has passed and dropped since it was added."""
    rows = json.loads(read([iproute2 + "/bin/tc", "-s", "-j", "qdisc", "show", "dev", DEVICE]) or "[]")
    netem = [row for row in rows if row["kind"] == "netem"]
    if len(netem) != 1:
        raise RuntimeError(f"The pair's loopback has {len(netem)} netem qdiscs, not one.")
    return {"passed": netem[0]["packets"], "dropped": netem[0]["drops"]}


def difference(before: dict, after: dict) -> dict:
    return {name: after[name] - before[name] for name in ("passed", "dropped")}


# moonlight-common-c's own lines of loss and recovery (RtpVideoQueue.c,
# VideoDepacketizer.c, ControlStream.c, AudioStream.c, RtpAudioQueue.c). Its
# release build logs no successful FEC recovery: a loss that FEC repaired
# shows only as netem's drop with no line here.
UNRECOVERABLE = re.compile(r"Unrecoverable frame (\d+)")
DROPPED_FRAMES = re.compile(r"Network dropped (?:1 frame \(|(\d+) frames \()")


def recovery(log: str) -> dict:
    """What the viewer's core logged of loss and recovery in one start."""
    dropped = 0
    for match in DROPPED_FRAMES.finditer(log):
        dropped += int(match.group(1)) if match.group(1) else 1
    return {
        "unrecoverable_frames": len(set(UNRECOVERABLE.findall(log))),
        "network_dropped_frames": dropped,
        "rfi_requests": log.count("Invalidate reference frame request sent"),
        "idr_requests": log.count("IDR frame request sent"),
        "audio_dropped": log.count("Network dropped audio data"),
        "audio_unrecovered_blocks": log.count("Unable to recover audio data block"),
    }
