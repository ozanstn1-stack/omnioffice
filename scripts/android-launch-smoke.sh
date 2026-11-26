#!/bin/sh
# Android launch smoke test (run on the CI emulator after the instrumented
# suite): installs the debug APK, starts the app like a user would, and
# passes once the frontend has run and called into Rust - the Settings store
# logs the engine status ("engines: ...") to frontend.log at start-up.
#
# Why not an instrumentation test: instrumentation shares the app process,
# and when its activity is destroyed (ActivityScenario.close, or the runner
# finishing activities after every test) Tauri ends the process with exit(),
# which took the test runner down with it ("Process crashed"). Here the app
# runs in its own process, so a native-library load failure, a crash in
# onCreate or a frontend that never starts is still caught.
set -eu

PKG=io.github.ozanstn1.pdfswissarmyknife
APK=$(find src-tauri/gen/android/app/build/outputs/apk/universal/debug -name '*.apk' | head -n 1)
if [ -z "$APK" ]; then
  echo "launch smoke: no universal debug APK found" >&2
  exit 1
fi

adb install -r "$APK" > /dev/null
adb logcat -c
adb shell am start -W -n "$PKG/.MainActivity"

# adb joins its arguments into one remote command line, so keep this to a
# plain command (a quoted `sh -c '...'` loses its quotes on the way). The app
# log directory is <data dir>/logs on Android.
frontend_log() {
  adb shell run-as "$PKG" cat logs/frontend.log 2> /dev/null || true
}

attempt=0
empty_pids=0
while [ "$attempt" -lt 60 ]; do
  pid=$(timeout 15 adb shell pidof "$PKG" 2>/dev/null | tr -d '\r' || true)
  if [ -z "$pid" ]; then
    empty_pids=$((empty_pids + 1))
    if [ "$empty_pids" -ge 3 ]; then
      echo "launch smoke: the app process exited (adb state: $(timeout 5 adb get-state 2>/dev/null || echo unavailable))" >&2
      frontend_log | tail -n 20 >&2 || true
      timeout 15 adb logcat -d -v threadtime > android-logcat.txt 2>/dev/null || true
      grep -E "AndroidRuntime|FATAL|F DEBUG|panicked|RustStdoutStderr|chromium|Tauri" android-logcat.txt | tail -n 150 >&2 || true
      exit 1
    fi
    attempt=$((attempt + 1))
    sleep 2
    continue
  fi
  empty_pids=0
  if frontend_log | grep -q "engines:"; then
    echo "launch smoke: PASS - the frontend started and reached the Rust side"
    frontend_log | tail -n 5
    if timeout 15 adb shell dumpsys activity activities 2>/dev/null | grep -q "$PKG"; then
      echo "launch smoke: PASS - verified MainActivity is registered and running"
    fi
    timeout 15 adb shell am force-stop "$PKG" 2>/dev/null || true
    exit 0
  fi
  attempt=$((attempt + 1))
  sleep 2
done

echo "launch smoke: the frontend did not start within 120 s (adb state: $(timeout 5 adb get-state 2>/dev/null || echo unavailable))" >&2
frontend_log >&2
timeout 15 adb logcat -d -v threadtime > android-logcat.txt 2>/dev/null || true
grep -E "AndroidRuntime|FATAL|F DEBUG|chromium|RustStdoutStderr|Tauri" android-logcat.txt | tail -n 150 >&2 || true
exit 1
