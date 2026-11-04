#!/usr/bin/env bash
# Interleaved press-row runs on the RG353M: N rounds, each variant once per round.
# Per run: frames over, p95, and korrid's and dbus's CPU over the run (from /proc/PID/stat),
# plus a control window of the same length with no bench, in the same ssh session.
# Usage: run10.sh OUT N SURFACE... (each SURFACE a package dir name in /tmp/kb2/packages)
set -u
out=$1; n=$2; shift 2
host=10.42.0.1
ssh_() { ssh -o BatchMode=yes -o ConnectTimeout=10 -p 2222 "root@$host" "$@"; }
games=$(ssh_ 'find /sys/fs/cgroup/korri.slice/korri-games.slice -name cgroup.procs -exec cat {} + 2>/dev/null | wc -l')
[ "$games" != 0 ] && { echo "SKIP: a game is running" | tee "$out"; exit 0; }
zstd -dc /tmp/shell2-dev/kb2.tar.zst | ssh_ 'rm -rf /tmp/kb2 && tar -C /tmp -xf -'
trap "ssh -o BatchMode=yes -p 2222 root@$host 'rm -rf /tmp/kb2'" EXIT
cat > /tmp/shell2-dev/dev10.sh <<'DEV'
set -u
cpu() { # utime+stime ticks of the newest process named $1
  p=$(pgrep -nx "$1"); [ -z "$p" ] && { echo 0; return; }
  awk '{print $14+$15}' /proc/$p/stat
}
saved=/tmp/kb2/saved
: > $saved
for p in /sys/devices/system/cpu/cpufreq/policy*; do echo "$p $(cat $p/scaling_max_freq)" >> $saved; done
restore() { while read -r p f; do echo "$f" > "$p/scaling_max_freq"; done < $saved; }
trap restore EXIT; trap 'exit 1' HUP INT TERM
for p in /sys/devices/system/cpu/cpufreq/policy*; do cat $p/cpuinfo_max_freq > $p/scaling_max_freq; done
for round in $(seq 1 $N); do
  for s in $SURFACES; do
    k0=$(cpu korrid); d0=$(cpu dbus-daemon); t0=$(date +%s.%N); l=$(cut -d' ' -f1 /proc/loadavg)
    tis0=$(cat /sys/devices/system/cpu/cpufreq/policy0/stats/time_in_state 2>/dev/null | tr '\n' ' ')
    r=$(cd /tmp/kb2 && setpriv --reuid=65534 --regid=65534 --clear-groups nice -n 10 ./run.sh bench --packages /tmp/kb2/packages --surface $s --frames 600 --pref zone.look=modern --press right,right,down,up,left,left 2>&1)
    t1=$(date +%s.%N); k1=$(cpu korrid); d1=$(cpu dbus-daemon); temp=$(cat /sys/class/thermal/thermal_zone0/temp); freq=$(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq); tis1=$(cat /sys/devices/system/cpu/cpufreq/policy0/stats/time_in_state 2>/dev/null | tr '\n' ' ')
    tis=$(echo "$tis0|$tis1" | awk -F'|' '{n=split($1,a," "); split($2,b," "); s=""; for(i=1;i<=n;i+=2){d=b[i+1]-a[i+1]; if(d>0) s=s a[i]/1000 ":" d ","} print s}')
    over=$(echo "$r" | sed -n 's/frames over 16.7 ms: \([0-9]*\) of.*/\1/p'); p95=$(echo "$r" | awk '/^frame total/{print $4}')
    dur=$(awk "BEGIN{print $t1 - $t0}")
    # Control: the same length, no bench.
    c0=$(cpu korrid); e0=$(cpu dbus-daemon); sleep $(printf %.0f $dur); c1=$(cpu korrid); e1=$(cpu dbus-daemon)
    echo "RUN round=$round surface=$s load=$l over=$over p95=$p95 secs=$dur korrid_ticks=$((k1-k0)) dbus_ticks=$((d1-d0)) control_korrid_ticks=$((c1-c0)) control_dbus_ticks=$((e1-e0)) temp=$temp max=$freq time_in_state=$tis"
  done
done
DEV
{ cat /tmp/shell2-dev/dev10.sh; } | ssh_ "cat > /tmp/kb2/dev10.sh && N=$n SURFACES='$*' sh /tmp/kb2/dev10.sh" | tee "$out"
python3 - "$out" <<'PY'
import sys,re,statistics
rows=[dict(kv.split('=') for kv in l.split()[1:]) for l in open(sys.argv[1]) if l.startswith('RUN')]
by={}
for r in rows: by.setdefault(r['surface'],[]).append(r)
with open(sys.argv[1],'a') as f:
    for s,rs in by.items():
        o=sorted(int(r['over']) for r in rs)
        q=statistics.quantiles(o,n=4) if len(o)>=4 else [o[0],statistics.median(o),o[-1]]
        k=[int(r['korrid_ticks'])/float(r['secs']) for r in rs]; c=[int(r['control_korrid_ticks'])/float(r['secs']) for r in rs]
        f.write(f"# {s}: over {o} median {statistics.median(o)} IQR {q[0]}-{q[2]}; korrid ticks/s bench {statistics.median(k):.1f} vs control {statistics.median(c):.1f}\n")
PY
tail -3 "$out"
