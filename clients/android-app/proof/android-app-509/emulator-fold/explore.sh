#!/usr/bin/env bash
# fold-509: can aka's Android 12 emulator simulate a fold so that the default
# display's physical mode changes? Two boots: a resizable profile, then a
# foldable profile. Holds /var/tmp/korri-turns.lock. Runs on aka as root.
set -u
export PATH=/run/wrappers/bin:/run/current-system/sw/bin
ROOT=/var/tmp/fold-509
OUT=$ROOT/explore
SDK=/nix/store/4mjwv048pgwyx819y8vxmz5smkxzal2l-androidsdk/libexec/android-sdk
export JAVA_HOME=/nix/store/4dqvcqjnhj6n29r6sh5wwrz58540gbfn-openjdk-17.0.20.1+1
export PATH=$JAVA_HOME/bin:$PATH
PORT=5684
export ANDROID_ADB_SERVER_PORT=5184
export ANDROID_SDK_ROOT=$SDK ANDROID_HOME=$SDK ANDROID_USER_HOME=$OUT/home ANDROID_EMULATOR_HOME=$OUT/home ANDROID_AVD_HOME=$OUT/home/avd
ADB="$SDK/platform-tools/adb -s emulator-$PORT"
mkdir -p "$OUT/home/avd"
. "$ROOT/lock.sh"
EMU=
take_turn "fold-509 emulator fold exploration" "$OUT/host.txt" || exit 3
trap 'halt; release_turn' EXIT

facts() { # NAME
  {
    echo "== wm size"; $ADB shell wm size
    echo "== wm density"; $ADB shell wm density
    echo "== dumpsys display (modes, sizes)"
    $ADB shell dumpsys display | grep -E "mBaseDisplayInfo|mOverrideDisplayInfo|DisplayDeviceInfo|mActiveModeId|mActiveSfDisplayMode|mDefaultModeId|supportedModes|  mode |mCurrentLayout|mDeviceState|mPendingDeviceState" | cut -c1-900
    echo "== device_state"; $ADB shell cmd device_state print-states 2>&1 | head -20; $ADB shell cmd device_state print-state 2>&1 | head -3
    echo "== SurfaceFlinger display modes"; $ADB shell dumpsys SurfaceFlinger --display-id 2>&1 | head -5
    $ADB shell dumpsys SurfaceFlinger | grep -iE "DisplayModes|activeMode|Display [0-9]+ .*HWC|resolution|^ *id=|{id=" | head -20
  } > "$OUT/$1.txt" 2>&1
}

boot() { # NAME, then config lines on stdin
  local name=$1 avd=$OUT/home/avd/$1.avd
  rm -rf "$avd" "$OUT/home/avd/$name.ini"
  echo no | "$SDK"/cmdline-tools/*/bin/avdmanager create avd --force -n "$name" \
    -k "system-images;android-31;google_apis;x86_64" -p "$avd" >"$OUT/$name-avdmanager.log" 2>&1 || return 1
  cat >> "$avd/config.ini"
  cp "$avd/config.ini" "$OUT/$name-config.ini"
  setsid nice -n 10 "$SDK/emulator/emulator" -avd "$name" -port $PORT -no-window -no-boot-anim -no-audio \
    -wipe-data -no-snapshot -no-metrics -gpu swiftshader_indirect -cores 2 -memory 2048 -accel on \
    >"$OUT/$name-emulator.log" 2>&1 &
  EMU=$!
  local until=$(( SECONDS + 600 ))
  while [ $SECONDS -lt $until ]; do
    kill -0 $EMU 2>/dev/null || { echo "$name: the emulator exited" >>"$OUT/host.txt"; return 1; }
    [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && { echo "$(date -u +%T) $name booted" >>"$OUT/host.txt"; sleep 15; return 0; }
    sleep 3
  done
  echo "$name: no boot in 10 minutes" >>"$OUT/host.txt"; return 1
}

halt() {
  [ -n "$EMU" ] || return 0
  $ADB emu kill >/dev/null 2>&1
  for _ in $(seq 60); do kill -0 "$EMU" 2>/dev/null || break; sleep 1; done
  kill -0 "$EMU" 2>/dev/null && kill -9 -- "-$EMU" 2>/dev/null
  "$SDK/platform-tools/adb" kill-server >/dev/null 2>&1
  echo "$(date -u +%T) emulator stopped; left: $(emulators)" >>"$OUT/host.txt"
  EMU=
}

emu() { echo "\$ adb emu $*" >>"$OUT/commands.txt"; $ADB emu "$@" >>"$OUT/commands.txt" 2>&1; sleep 6; }

common="hw.lcd.density=420
hw.ramSize=2048
hw.cpu.ncore=2
hw.gpu.enabled=yes
hw.gpu.mode=swiftshader_indirect
hw.keyboard=yes
hw.audioInput=no
hw.audioOutput=no
hw.camera.back=none
hw.camera.front=none
fastboot.forceColdBoot=yes"

# 1. The resizable profile (Android Studio's "Resizable (Experimental)").
if boot resizable <<EOF2
$common
hw.lcd.width=1080
hw.lcd.height=2400
hw.resizable.configs=phone-0-1080-2400-420, foldable-1-2208-1840-420, tablet-2-1920-1200-240, desktop-3-1920-1080-160
EOF2
then
  facts resizable-0-boot
  emu resize-display 1; facts resizable-1-foldable
  emu resize-display 2; facts resizable-2-tablet
  emu resize-display 0; facts resizable-3-phone-again
  grep -iE "resizable|resize" "$OUT/resizable-emulator.log" | head -20 > "$OUT/resizable-emulator-lines.txt"
fi
halt

# 2. The foldable profile (Android Studio's 7.6" Fold-in with outer display).
if boot foldable <<EOF2
$common
hw.lcd.width=1768
hw.lcd.height=2208
hw.sensor.hinge=yes
hw.sensor.hinge.count=1
hw.sensor.hinge.type=1
hw.sensor.hinge.sub_type=1
hw.sensor.hinge.ranges=0-180
hw.sensor.hinge.defaults=180
hw.sensor.hinge.areas=884-0-1-2208
hw.sensor.posture_list=1,2,3
hw.sensor.hinge_angles_posture_definitions=0-30, 30-150, 150-180
hw.sensor.hinge.fold_to_displayRegion.0.1_at_posture=1
hw.displayRegion.0.1.xOffset=0
hw.displayRegion.0.1.yOffset=0
hw.displayRegion.0.1.width=884
hw.displayRegion.0.1.height=2208
EOF2
then
  facts foldable-0-boot
  emu fold; facts foldable-1-folded
  emu unfold; facts foldable-2-unfolded
  echo "\$ wm size 884x2208" >>"$OUT/commands.txt"; $ADB shell wm size 884x2208 >>"$OUT/commands.txt" 2>&1; sleep 4; facts foldable-3-wm-size
  $ADB shell wm size reset; sleep 2
  grep -iE "fold|posture|hinge" "$OUT/foldable-emulator.log" | head -20 > "$OUT/foldable-emulator-lines.txt"
fi
halt
echo "$(date -u +%T) load=$(cut -d' ' -f1 /proc/loadavg) k10temp=$(tctl)C done" >>"$OUT/host.txt"
