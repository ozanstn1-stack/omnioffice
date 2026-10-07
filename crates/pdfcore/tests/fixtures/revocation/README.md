# Revocation test fixtures

A throwaway PKI for `crates/pdfcore/tests/revocation.rs`, generated once with
the OpenSSL 3.0 command line. Every key here is a test key that protects
nothing; the `.test` host names are reserved and never resolve, and the tests
never touch the network (a closure serves these files instead).

| File | What it is |
| --- | --- |
| `ca.der` | Self-signed RSA-2048 CA, `keyCertSign, cRLSign` |
| `signer.der`, `signer-key.der` | P-256 signer (serial 0x1000) with AIA `OCSP;URI:http://ocsp.omnioffice.test/` and CRL distribution points `http://crl.omnioffice.test/ca.crl` and an `ldap://` one (which must be skipped) |
| `revoked.der`, `revoked-key.der` | Second P-256 signer (serial 0x1001), revoked with reason `keyCompromise` |
| `responder.der` | Delegated OCSP responder issued by the CA, EKU `OCSPSigning` |
| `rogue.der` | Self-signed impostor with the same subject and EKU as the responder |
| `ocsp-request-signer.der` | OpenSSL's request for `signer.der` (SHA-1 CertID, no nonce) |
| `ocsp-good.der` | `good` for the signer, signed by `responder.der`, responder ID by name |
| `ocsp-revoked.der` | `revoked` for the revoked signer, signed by `responder.der` |
| `ocsp-good-by-ca.der` | `good` for the signer, signed by the CA itself, responder ID by key hash |
| `ocsp-rogue.der` | `good` for the signer, signed by `rogue.der` (must be rejected) |
| `ocsp-unknown.der` | `unknown` for the signer (the responder's index is empty) |
| `ca.crl` | DER CRL listing the revoked signer |

Every OCSP response and the CRL have `thisUpdate` 2026-10-07T06:49:41Z (Unix
1791355781) and `nextUpdate` 2126-09-13T06:49:41Z (Unix 4944955781); the tests
pass the evaluation time explicitly, so they do not depend on the clock.

## Regenerating

`generate.sh` holds the exact commands and `openssl.cnf` the CA and extension
profiles. Run it from a scratch directory that contains `openssl.cnf`:

```sh
mkdir -p /tmp/pki && cp openssl.cnf generate.sh /tmp/pki/ && bash /tmp/pki/generate.sh /tmp/pki "$PWD"
```

The commands, in short:

```sh
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out ca.key
openssl req -x509 -new -key ca.key -subj "/CN=OmniOffice Test Revocation CA" -days 36500 -sha256 \
  -config openssl.cnf -extensions v3_ca -out ca.pem
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out signer.key   # also revoked, responder, rogue
openssl req -new -key signer.key -subj "/CN=OmniOffice Test Signer" -config openssl.cnf -out signer.csr
openssl ca -batch -config openssl.cnf -cert ca.pem -keyfile ca.key -extensions v3_signer -in signer.csr -out signer.pem
openssl ca -batch -config openssl.cnf -cert ca.pem -keyfile ca.key -extensions v3_responder -in responder.csr -out responder.pem
openssl req -x509 -new -key rogue.key -subj "/CN=OmniOffice Test OCSP Responder" -days 36500 -config openssl.cnf -extensions v3_rogue -out rogue.pem
openssl ca -config openssl.cnf -cert ca.pem -keyfile ca.key -revoke revoked.pem -crl_reason keyCompromise
openssl ca -config openssl.cnf -cert ca.pem -keyfile ca.key -gencrl -crldays 36500 -out crl.pem
openssl ocsp -issuer ca.pem -cert signer.pem -no_nonce -reqout ocsp-request-signer.der
openssl ocsp -index index.txt -CA ca.pem -rsigner responder.pem -rkey responder.key \
  -reqin ocsp-request-signer.der -respout ocsp-good.der -ndays 36500
openssl ocsp -index index.txt -CA ca.pem -rsigner ca.pem -rkey ca.key -resp_key_id \
  -reqin ocsp-request-signer.der -respout ocsp-good-by-ca.der -ndays 36500
openssl x509 -in signer.pem -outform DER -out signer.der
openssl pkcs8 -topk8 -nocrypt -in signer.key -outform DER -out signer-key.der
openssl crl -in crl.pem -outform DER -out ca.crl
```

Regenerating changes every key, date and signature: update `FIXTURE_TIME`
and `NEXT_UPDATE` in the test afterwards (`openssl ocsp -respin ocsp-good.der
-resp_text -noverify` prints both).
