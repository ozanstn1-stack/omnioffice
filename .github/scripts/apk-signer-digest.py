#!/usr/bin/env python3
"""Print the SHA-256 digest of the certificate that signed an APK.

`apksigner` is the obvious tool, but it is not on PATH on the runner and
building a JVM invocation into the gate just to read a certificate is worse
than parsing the signature block: the APK Signing Block format is stable and
the v2/v3 record we need sits in the first signer. Cross-checked against
`openssl x509 -fingerprint -sha256` on a release APK (same digest).

    python3 .github/scripts/apk-signer-digest.py app.apk
"""
import hashlib, struct, sys, zipfile

PAIR_V2, PAIR_V3 = 0x7109871A, 0xF05368C0

def uint32(buf, at):
    return struct.unpack("<I", buf[at:at + 4])[0], at + 4

def chunk(buf, at):
    length, at = uint32(buf, at)
    return buf[at:at + length], at + length

def pairs(data):
    magic = b"APK Sig Block 42"
    at = data.rfind(magic)
    if at < 0:
        raise SystemExit("no APK Signing Block in this file")
    size = struct.unpack("<Q", data[at - 8:at])[0]
    start = at + 8 - size
    if struct.unpack("<Q", data[start:start + 8])[0] != size:
        raise SystemExit("the APK Signing Block's two size fields disagree")
    offset, end, found = start + 8, at - 8, {}
    while offset < end:
        length = struct.unpack("<Q", data[offset:offset + 8])[0]
        pair_id = struct.unpack("<I", data[offset + 8:offset + 12])[0]
        found[pair_id] = data[offset + 12:offset + 12 + length - 4]
        offset += 8 + length
    return found

def certificate(block):
    signers, _ = chunk(block, 0)
    signer, _ = chunk(signers, 0)
    signed_data, _ = chunk(signer, 0)
    _, at = chunk(signed_data, 0)          # digests
    certificates, _ = chunk(signed_data, at)
    der, _ = chunk(certificates, 0)
    return der

def v1_digest(path):
    """The v1 (JAR) signature file, hashed.

    A debug-signed build carries no APK Signing Block, so there is no v2/v3
    certificate record to read. The PKCS#7 file under META-INF embeds the
    certificate, and two packages signed with the same key carry the same
    bytes - which is exactly what "one signing identity" means here. Parsing
    the PKCS#7 to pull the certificate out would need an ASN.1 decoder for no
    extra certainty.
    """
    with zipfile.ZipFile(path) as archive:
        names = [
            name
            for name in archive.namelist()
            if name.upper().startswith("META-INF/") and name.upper().endswith((".RSA", ".DSA", ".EC"))
        ]
        if not names:
            raise SystemExit(f"{path}: no APK Signing Block and no META-INF signature file")
        return hashlib.sha256(archive.read(sorted(names)[0])).hexdigest()

for path in sys.argv[1:]:
    data = open(path, "rb").read()
    if b"APK Sig Block 42" not in data:
        print(f"{v1_digest(path)}  {path}  v1")
        continue
    found = pairs(data)
    for scheme, pair_id in (("v2", PAIR_V2), ("v3", PAIR_V3)):
        if pair_id in found:
            der = certificate(found[pair_id])
            if not der.startswith(b"0\x82"):
                raise SystemExit(f"{path}: the {scheme} signer certificate is not DER")
            print(f"{hashlib.sha256(der).hexdigest()}  {path}  {scheme}")
            break
    else:
        print(f"{v1_digest(path)}  {path}  v1")
