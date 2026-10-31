#!/bin/sh
# Boundary checks; the device runs are the integration tests.
set -eu
cd "$(dirname "$0")"
expect_exit() {
    expected=$1
    shift
    set +e
    "$@" >/dev/null 2>&1
    actual=$?
    set -e
    [ "$actual" -eq "$expected" ] || {
        printf 'expected exit %s, got %s: %s\n' "$expected" "$actual" "$*" >&2
        exit 1
    }
}
expect_exit 2 sh measure.sh
expect_exit 2 sh measure.sh relative-cgroup
expect_exit 1 sh measure.sh /does-not-exist-zone-spike
expect_exit 2 sh measure.sh /unused ignored.csv 0
expect_exit 2 sh measure.sh /unused ignored.csv -1
expect_exit 2 sh measure.sh /unused ignored.csv abc
expect_exit 2 sh measure.sh /unused ignored.csv 99999999999999999999
expect_exit 2 bash bundle.sh
printf 'boundary tests passed\n'
