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
# A pull request builds without a keystore, so its package is unsigned by design;
# every other run must be signed, and that is what protects a release.
require_signature=0
[ "${2:-}" = "--require-signature" ] && require_signature=1
version=$(node -p "require('./src-tauri/tauri.conf.json').version")
failures=0
fail() {
  echo "::error::$1"
  failures=$((failures + 1))
}

# ABI -> the ELF shape `file` must report for its libraries. `file` spells the
# 64-bit machine both "AArch64" and "ARM aarch64" depending on its magic table,
# so the match is on a lowercased string and the bit width tells the two ABIs
# apart.
declare -A machine=( [arm64-v8a]="aarch64" [armeabi-v7a]="arm" )
declare -A bits=( [arm64-v8a]="elf 64-bit" [armeabi-v7a]="elf 32-bit" )
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
      described=$(file -b "$sample" | tr '[:upper:]' '[:lower:]')
      case "$described" in
        *"${bits[$abi]}"*"${machine[$abi]}"*) : ;;
        *) fail "$file: ${libraries[0]} is not a ${bits[$abi]} ${machine[$abi]} object ($described)" ;;
      esac
    fi
    rm -rf "$scratch"
  done
done

# The same identity has to sign both APKs, or an update replaces the app. The
# digest comes from the signature block itself (apk-signer-digest.py): apksigner
# is not on PATH on the runner, and a check that silently skips itself is worse
# than no check.
script_dir=$(cd "$(dirname "$0")" && pwd)
digests=()
unsigned=0
for abi in "${!machine[@]}"; do
  file="$dir/OmniOffice-Android-$version-$abi.apk"
  [ -f "$file" ] || continue
  if ! digest=$(python3 -I "$script_dir/apk-signer-digest.py" "$file" 2>&1); then
    case "$digest" in
      *"no APK Signing Block and no META-INF signature file"*)
        unsigned=$((unsigned + 1))
        if [ "$require_signature" -eq 1 ]; then
          fail "$file is unsigned, and this run was expected to produce a signed package"
        fi
        continue
        ;;
    esac
    fail "${digest#"$file: "}"
    continue
  fi
  digest=${digest%% *}
  echo "signing certificate ($abi): $digest"
  digests+=("$digest")
done
if [ "${#digests[@]}" -gt 1 ] && [ "$(printf '%s\n' "${digests[@]}" | sort -u | wc -l)" -ne 1 ]; then
  fail "the APKs are signed with different certificates"
fi
if [ "${#digests[@]}" -gt 0 ] && [ "$unsigned" -gt 0 ]; then
  fail "one APK is signed and the other is not: the pair does not share an identity"
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
artifacts_count=$(ls "$dir"/*.apk "$dir"/*.aab | wc -l)
if [ "$unsigned" -gt 0 ]; then
  # Said out loud: a pull request has no keystore, so identity is not claimed.
  echo "ABI matrix OK: $artifacts_count artifacts, two ABIs (unsigned build: the signing identity was not compared)."
else
  echo "ABI matrix OK: $artifacts_count artifacts, two ABIs, one signing identity."
fi
