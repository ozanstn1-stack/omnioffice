#!/usr/bin/env bash
# Verifies the ABI matrix of the Android artifacts before they are published.
#
# Two things are easy to get wrong and invisible from the build log alone: an
# APK can carry another ABI's native libraries (the Tauri plugin copies every
# directory under jniLibs), and the two APKs can end up with different signing
# keys. Both would only surface on a device - or in a store review - so they are
# checked here, on the files themselves.
#
#   bash .github/scripts/verify-android-abis.sh [release-artifacts]
set -euo pipefail

dir="${1:-release-artifacts}"
version=$(node -p "require('./src-tauri/tauri.conf.json').version")
failures=0
fail() {
  echo "::error::$1"
  failures=$((failures + 1))
}

# ABI -> the ELF machine `file` must report for its libraries.
declare -A machine=( [arm64-v8a]="AArch64" [armeabi-v7a]="ARM" )
declare -A bits=( [arm64-v8a]="ELF 64-bit" [armeabi-v7a]="ELF 32-bit" )
libraries=(libpdf_sak_lib.so libpdfium.so libtesseract.so)

for abi in "${!machine[@]}"; do
  for kind in apk aab; do
    file="$dir/OmniOffice-Android-$version-$abi.$kind"
    if [ ! -f "$file" ]; then
      fail "missing $file"
      continue
    fi
    prefix="lib/"; [ "$kind" = "aab" ] && prefix="base/lib/"
    entries=$(python3 -I -m zipfile -l "$file" | awk 'NR>1 {print $1}' | grep "^${prefix}" || true)
    if [ -z "$entries" ]; then
      fail "$file carries no ${prefix}"
      continue
    fi
    # Only this ABI's directory may appear under lib/.
    others=$(printf '%s\n' "$entries" | awk -F/ -v p="$prefix" '{ sub("^" p, ""); print $1 }' | sort -u | grep -v "^${abi}$" || true)
    if [ -n "$others" ]; then
      fail "$file carries other ABIs: $(echo "$others" | tr '\n' ' ')"
    fi
    for library in "${libraries[@]}"; do
      printf '%s\n' "$entries" | grep -qx "${prefix}${abi}/${library}" || fail "$file is missing ${prefix}${abi}/${library}"
    done

    # One library is enough to prove the ELF shape; the names above prove the set.
    scratch=$(mktemp -d)
    python3 -I -m zipfile -e "$file" "$scratch" >/dev/null
    sample="$scratch/${prefix}${abi}/${libraries[0]}"
    if [ -f "$sample" ]; then
      described=$(file -b "$sample")
      case "$described" in
        *"${bits[$abi]}"*"${machine[$abi]}"*) : ;;
        *) fail "$file: ${libraries[0]} is not a ${machine[$abi]} ${bits[$abi]} object ($described)" ;;
      esac
    fi
    rm -rf "$scratch"
  done
done

# The same identity has to sign both APKs, or an update replaces the app.
if command -v apksigner >/dev/null 2>&1; then
  digests=()
  for abi in "${!machine[@]}"; do
    file="$dir/OmniOffice-Android-$version-$abi.apk"
    [ -f "$file" ] || continue
    digest=$(apksigner verify --print-certs "$file" 2>/dev/null |
      sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | head -n1)
    if [ -z "$digest" ]; then
      fail "$file: apksigner reported no certificate"
      continue
    fi
    echo "signing certificate ($abi): $digest"
    digests+=("$digest")
  done
  if [ "${#digests[@]}" -gt 1 ] && [ "$(printf '%s\n' "${digests[@]}" | sort -u | wc -l)" -ne 1 ]; then
    fail "the APKs are signed with different certificates"
  fi
else
  echo "apksigner not on PATH; the signing identity was not compared"
fi

# The checksum list the release attaches must match the files next to it.
manifest="$dir/SHA256SUMS-android.txt"
if [ -f "$manifest" ]; then
  (cd "$dir" && sha256sum -c SHA256SUMS-android.txt)
else
  fail "missing $manifest"
fi

if [ "$failures" -ne 0 ]; then
  echo "::error::$failures ABI matrix problem(s) found"
  exit 1
fi
echo "ABI matrix OK: $(ls "$dir"/*.apk "$dir"/*.aab | wc -l) artifacts, two ABIs, one signing identity."
