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
while [ "$attempt" -lt 60 ]; do
  if [ -z "$(adb shell pidof "$PKG" | tr -d '\r')" ]; then
    echo "launch smoke: the app process exited" >&2
    adb logcat -d -v threadtime | grep -E "AndroidRuntime|FATAL|F DEBUG|panicked|RustStdoutStderr" | tail -n 150 >&2
    exit 1
  fi
  if frontend_log | grep -q "engines:"; then
    echo "launch smoke: PASS - the frontend started and reached the Rust side"
    frontend_log | tail -n 5
    adb shell am force-stop "$PKG"
    exit 0
  fi
  attempt=$((attempt + 1))
  sleep 2
done

echo "launch smoke: the frontend did not start within 120 s" >&2
frontend_log >&2
adb logcat -d -v threadtime | grep -E "AndroidRuntime|FATAL|F DEBUG|chromium|RustStdoutStderr|Tauri" | tail -n 150 >&2
exit 1
