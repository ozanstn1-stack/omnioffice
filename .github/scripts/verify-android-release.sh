#!/usr/bin/env bash
# Release identity checks for the Android catalogues (#502).
#
# Two things must hold for every APK that is attached to a GitHub release:
#
#   1. the version code is exactly major * 1_000_000 + minor * 1_000 + patch
#      (F-Droid and IzzyOnDroid order updates by this integer);
#   2. the APK is signed with the same certificate as the previous release
#      (a different key cannot update an installed app, and both catalogues
#      treat a changed key as a new app).
#
# Runs in the Android workflow on tag builds only, after the APKs are built.
set -euo pipefail

version=$(node -p "require('./src-tauri/tauri.conf.json').version")
apk="release-artifacts/OmniOffice-Android-${version}-arm64-v8a.apk"
if [ ! -f "$apk" ]; then
  echo "::error::release APK is missing: $apk"
  exit 1
fi

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$sdk" ] || [ ! -d "$sdk/build-tools" ]; then
  echo "::error::Android build-tools are not available (ANDROID_HOME/ANDROID_SDK_ROOT)"
  exit 1
fi
apksigner=$(find "$sdk/build-tools" -type f -name apksigner | sort -V | tail -n 1)
aapt=$(find "$sdk/build-tools" -type f -name aapt | sort -V | tail -n 1)
if [ -z "$apksigner" ]; then
  echo "::error::apksigner not found under $sdk/build-tools"
  exit 1
fi

certificate() {
  "$apksigner" verify --print-certs "$1" |
    awk -F': ' '/certificate SHA-256 digest/ {print $2; exit}' |
    tr '[:upper:]' '[:lower:]'
}

expected_code=$(node -e "const [a,b,c]='$version'.split('.').map(Number); console.log(a*1000000+b*1000+c)")
cert=$(certificate "$apk")
if [ -z "$cert" ]; then
  echo "::error::could not read the signing certificate of $apk"
  exit 1
fi
echo "APK version: $version"
echo "APK certificate SHA-256: $cert"

if [ -n "$aapt" ]; then
  code=$("$aapt" dump badging "$apk" | sed -n "s/.*versionCode='\\([0-9]*\\)'.*/\\1/p" | head -n 1)
  if [ "$code" != "$expected_code" ]; then
    echo "::error::APK versionCode $code does not match the formula ($expected_code)"
    exit 1
  fi
  echo "APK versionCode: $code"
else
  echo "aapt is not available; skipping the packaged versionCode check"
fi

# Compare with the newest earlier release (calendar order of the release list).
previous_tags=$(
  gh release list --exclude-drafts --exclude-pre-releases --limit 30 --json tagName --jq '.[].tagName' |
    grep -vx "v${version}" || true
)
previous_tag=$(printf '%s\n' "$previous_tags" | head -n 1 || true)
if [ -z "$previous_tag" ]; then
  echo "no earlier release to compare against; the certificate becomes the new baseline"
  exit 0
fi
asset=$(gh release view "$previous_tag" --json assets --jq '.assets[].name' | grep -E 'arm64-v8a\.apk$' | head -n 1 || true)
if [ -z "$asset" ]; then
  echo "warning::no arm64 APK on $previous_tag; signing continuity not compared"
  exit 0
fi
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT
gh release download "$previous_tag" --pattern "$asset" --dir "$workdir" >/dev/null
previous_cert=$(certificate "$workdir/$asset")
if [ "$previous_cert" != "$cert" ]; then
  echo "::error::signing key changed since $previous_tag ($previous_cert -> $cert); installed users cannot update"
  exit 1
fi
echo "Signing certificate matches the previous release ($previous_tag)."
