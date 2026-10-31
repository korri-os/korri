#!/bin/sh
# Root is normally needed to read every process's smaps_rollup.
# Usage: sh measure.sh /system.slice/UNIT [RAW_SAMPLES.csv] [SECONDS]
set -eu
[ "$#" -ge 1 ] && [ "$#" -le 3 ] || {
    echo "usage: $0 CGROUP [RAW_SAMPLES.csv] [SECONDS]" >&2
    exit 2
}
samples=${3:-60}
case $samples in ''|*[!0-9]*|?????*) echo 'seconds must be 1..3600' >&2; exit 2 ;; esac
[ "$samples" -ge 1 ] && [ "$samples" -le 3600 ] || {
    echo 'seconds must be 1..3600' >&2
    exit 2
}
case $1 in /*) group=/sys/fs/cgroup$1 ;; *) echo 'expected absolute cgroup path' >&2; exit 2 ;; esac
[ -r "$group/cgroup.procs" ] && [ -r "$group/cpu.stat" ] || {
    echo 'cgroup does not exist or is not readable' >&2
    exit 1
}
raw=${2:-samples.csv}
# Never overwrite prior evidence.
(set -C; : > "$raw") || exit 1
pids=$raw.pids
values=$raw.values
trap 'rm -f "$pids" "$values"' EXIT HUP INT TERM
mem_start=$(awk '/^MemAvailable:/ {print $2}' /proc/meminfo)
cpu_start=$(awk '$1 == "usage_usec" {print $2}' "$group/cpu.stat")
start=$(awk '{print $1}' /proc/uptime)
printf 'second,uptime_seconds,pss_kib,processes\n' >> "$raw"
i=1
while [ "$i" -le "$samples" ]; do
    # Schedule against monotonic uptime so smaps cost does not accumulate
    # to the measurement duration. Linux sleep accepts fractional seconds.
    now=$(awk '{print $1}' /proc/uptime)
    delay=$(awk -v start="$start" -v now="$now" -v i="$i" 'BEGIN {d=start+i-now; printf "%.2f", (d > 0 ? d : 0)}')
    sleep "$delay"
    # Include descendant cgroups as well as the unit's direct processes.
    find "$group" -name cgroup.procs -exec cat {} \; | sort -nu > "$pids"
    total=0
    count=0
    while IFS= read -r pid; do
        [ -n "$pid" ] || continue
        if pss=$(awk '$1 == "Pss:" {print $2}' "/proc/$pid/smaps_rollup" 2>/dev/null); then
            [ -n "$pss" ] || continue
            total=$((total + pss))
            count=$((count + 1))
        elif [ -d "/proc/$pid" ]; then
            echo "cannot read smaps_rollup for live pid $pid" >&2
            exit 1
        fi
    done < "$pids"
    [ "$count" -gt 0 ] || { echo 'cgroup has no readable processes' >&2; exit 1; }
    up=$(awk '{print $1}' /proc/uptime)
    printf '%s,%s,%s,%s\n' "$i" "$up" "$total" "$count" >> "$raw"
    i=$((i + 1))
done
end=$(awk '{print $1}' /proc/uptime)
cpu_end=$(awk '$1 == "usage_usec" {print $2}' "$group/cpu.stat")
mem_end=$(awk '/^MemAvailable:/ {print $2}' /proc/meminfo)
awk -F, 'NR > 1 {print $3}' "$raw" | sort -n > "$values"
awk -v begin="$start" -v end="$end" -v cpu_begin="$cpu_start" -v cpu_end="$cpu_end" \
    -v mem_begin="$mem_start" -v mem_end="$mem_end" -v n="$samples" '
    NR == int((n+1)/2) {a=$1} NR == int((n+2)/2) {b=$1} {max=$1}
    END {
        printf "samples=%d\nmedian_pss_kib=%.1f\nmax_pss_kib=%d\n", NR, (a+b)/2, max
        printf "elapsed_seconds=%.2f\ncpu_one_core_percent=%.3f\n", end-begin, (cpu_end-cpu_begin)/(end-begin)/10000
        printf "memavailable_start_kib=%d\nmemavailable_end_kib=%d\n", mem_begin, mem_end
    }' "$values"
printf 'raw_samples=%s\n' "$raw"
