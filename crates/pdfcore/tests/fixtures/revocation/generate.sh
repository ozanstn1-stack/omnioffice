#!/usr/bin/env bash
# Generates the revocation test fixtures. Usage: generate.sh <workdir> <outdir>
set -euo pipefail
work="$1"
out="$2"
cd "$work"
rm -rf newcerts index.txt* serial* crlnumber* empty-index.txt* ./*.pem ./*.key ./*.csr ./*.der
mkdir -p newcerts "$out"
: > index.txt
: > empty-index.txt
echo 1000 > serial
echo 01 > crlnumber

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out ca.key
openssl req -x509 -new -key ca.key -subj "/CN=OmniOffice Test Revocation CA" -days 36500 -sha256 \
  -config openssl.cnf -extensions v3_ca -out ca.pem
for name in signer revoked responder rogue; do
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$name.key"
done
openssl req -new -key signer.key -subj "/CN=OmniOffice Test Signer" -config openssl.cnf -out signer.csr
openssl req -new -key revoked.key -subj "/CN=OmniOffice Test Revoked Signer" -config openssl.cnf -out revoked.csr
openssl req -new -key responder.key -subj "/CN=OmniOffice Test OCSP Responder" -config openssl.cnf -out responder.csr
openssl ca -batch -config openssl.cnf -cert ca.pem -keyfile ca.key -extensions v3_signer -in signer.csr -out signer.pem -notext
openssl ca -batch -config openssl.cnf -cert ca.pem -keyfile ca.key -extensions v3_signer -in revoked.csr -out revoked.pem -notext
openssl ca -batch -config openssl.cnf -cert ca.pem -keyfile ca.key -extensions v3_responder -in responder.csr -out responder.pem -notext
# Same subject as the real responder, OCSP-signing EKU, but self-signed: it was
# never authorized by the CA.
openssl req -x509 -new -key rogue.key -subj "/CN=OmniOffice Test OCSP Responder" -days 36500 -sha256 \
  -config openssl.cnf -extensions v3_rogue -out rogue.pem

openssl ca -config openssl.cnf -cert ca.pem -keyfile ca.key -revoke revoked.pem -crl_reason keyCompromise
openssl ca -config openssl.cnf -cert ca.pem -keyfile ca.key -gencrl -crldays 36500 -out crl.pem

openssl ocsp -issuer ca.pem -cert signer.pem -no_nonce -reqout ocsp-request-signer.der
openssl ocsp -issuer ca.pem -cert revoked.pem -no_nonce -reqout ocsp-request-revoked.der
# Delegated responder (OCSP-signing certificate issued by the CA).
openssl ocsp -index index.txt -CA ca.pem -rsigner responder.pem -rkey responder.key \
  -reqin ocsp-request-signer.der -respout ocsp-good.der -ndays 36500 > /dev/null
openssl ocsp -index index.txt -CA ca.pem -rsigner responder.pem -rkey responder.key \
  -reqin ocsp-request-revoked.der -respout ocsp-revoked.der -ndays 36500 > /dev/null
# The CA answers itself, identified by key hash.
openssl ocsp -index index.txt -CA ca.pem -rsigner ca.pem -rkey ca.key -resp_key_id \
  -reqin ocsp-request-signer.der -respout ocsp-good-by-ca.der -ndays 36500 > /dev/null
# The self-signed impostor responder.
openssl ocsp -index index.txt -CA ca.pem -rsigner rogue.pem -rkey rogue.key \
  -reqin ocsp-request-signer.der -respout ocsp-rogue.der -ndays 36500 > /dev/null
# An index that does not know the certificate: status "unknown".
openssl ocsp -index empty-index.txt -CA ca.pem -rsigner responder.pem -rkey responder.key \
  -reqin ocsp-request-signer.der -respout ocsp-unknown.der -ndays 36500 > /dev/null

for name in ca signer revoked responder rogue; do
  openssl x509 -in "$name.pem" -outform DER -out "$out/$name.der"
done
for name in signer revoked; do
  openssl pkcs8 -topk8 -nocrypt -in "$name.key" -outform DER -out "$out/$name-key.der"
done
openssl crl -in crl.pem -outform DER -out "$out/ca.crl"
cp ocsp-request-signer.der ocsp-good.der ocsp-revoked.der ocsp-good-by-ca.der ocsp-rogue.der ocsp-unknown.der "$out/"
