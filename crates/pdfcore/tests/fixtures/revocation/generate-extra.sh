#!/usr/bin/env bash
# Generates the extra revocation fixtures (CRL partitions, issuers that are not
# CAs). Usage: generate-extra.sh <workdir> <outdir>
# <workdir> must contain openssl-extra.cnf. Test keys only.
set -euo pipefail
work="$1"
out="$2"
mkdir -p "$out"
cd "$work"
# The config expands $ENV::IDP_URI wherever it is parsed; only `crl` uses it.
export IDP_URI="http://unused.invalid/"

# new_pki <name> <ca extension profile>: a self-signed issuer, one P-256 signer
# it issued (serial 0x1000) and an empty database.
new_pki() {
  local name="$1" profile="$2"
  rm -rf "$name"
  mkdir -p "$name/newcerts"
  cp openssl-extra.cnf "$name/"
  (
    cd "$name"
    : > index.txt
    echo 1000 > serial
    echo 01 > crlnumber
    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out ca.key
    openssl req -x509 -new -key ca.key -subj "/CN=OmniOffice Test Extra $name" -days 36500 -sha256 \
      -config openssl-extra.cnf -extensions "$profile" -out ca.pem
    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out signer.key
    openssl req -new -key signer.key -subj "/CN=OmniOffice Test Signer $name" -config openssl-extra.cnf -out signer.csr
    openssl ca -batch -config openssl-extra.cnf -cert ca.pem -keyfile ca.key -extensions v3_signer \
      -in signer.csr -out signer.pem -notext
    openssl x509 -in ca.pem -outform DER -out "$out/$name-ca.der"
    openssl x509 -in signer.pem -outform DER -out "$out/$name-signer.der"
  )
}

# crl <pki> <file> [idp uri]: a CRL, with an IssuingDistributionPoint when a
# URI is given.
crl() {
  local name="$1" file="$2" idp="${3:-}"
  (
    cd "$name"
    if [ -n "$idp" ]; then
      IDP_URI="$idp" openssl ca -config openssl-extra.cnf -cert ca.pem -keyfile ca.key -gencrl -crldays 36500 \
        -out crl.pem
    else
      sed '/^crl_extensions/d' openssl-extra.cnf > plain.cnf
      openssl ca -config plain.cnf -cert ca.pem -keyfile ca.key -gencrl -crldays 36500 -out crl.pem
    fi
    openssl crl -in crl.pem -outform DER -out "$out/$file"
  )
}

# ocsp <pki> <file>: "good" for the signer, signed by the issuer itself.
ocsp() {
  local name="$1" file="$2"
  (
    cd "$name"
    openssl ocsp -issuer ca.pem -cert signer.pem -no_nonce -reqout req.der
    openssl ocsp -index index.txt -CA ca.pem -rsigner ca.pem -rkey ca.key -resp_key_id \
      -reqin req.der -respout "$out/$file" -ndays 36500 > /dev/null
  )
}

new_pki idp v3_ca
crl idp idp-match.crl "http://crl.omnioffice.test/part1.crl"
crl idp idp-other.crl "http://crl.omnioffice.test/part2.crl"
crl idp idp-none.crl

new_pki notca v3_not_ca
crl notca notca.crl
ocsp notca notca-ocsp.der

new_pki nokcs v3_no_keycertsign
crl nokcs nokcs.crl
ocsp nokcs nokcs-ocsp.der
