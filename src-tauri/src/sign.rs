//! Real PDF digital signature commands.
//!
//! All cryptography lives in `pdfcore::sign`; this module only bridges it to
//! the UI: it reads files, optionally unlocks a certificate from the Windows
//! certificate store, writes the result atomically and re-verifies the file it
//! produced before reporting success.
//!
//! Secrets (PFX passwords, exported private keys) are held in memory only and
//! are never logged or written to disk.

use pdfcore::error::{PdfError, PdfResult};
use pdfcore::sign::{self, SignatureInfo, SignatureReport, SignOptions};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// UI facing signing options (camelCase on the wire, matching the frontend).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignOptionsDto {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default)]
    pub rect: Option<[f32; 4]>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub contact: String,
    #[serde(default = "default_true")]
    pub appearance: bool,
    #[serde(default)]
    pub signer_name: Option<String>,
}

fn default_page() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

impl SignOptionsDto {
    fn into_options(self) -> SignOptions {
        SignOptions {
            page: self.page.max(1),
            rect: self.rect,
            reason: self.reason,
            location: self.location,
            contact: self.contact,
            appearance: self.appearance,
            signer_name: self.signer_name,
        }
    }
}

/// Result of a successful signing operation: the written path plus the
/// verification report of the produced signature (never an assumed success).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignResultDto {
    pub output: String,
    pub signature: SignatureInfo,
}

/// One personal certificate from the Windows certificate store.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateSummaryDto {
    pub index: usize,
    pub subject: String,
    pub issuer: String,
    pub serial_hex: String,
    pub not_before: String,
    pub not_after: String,
    pub expired: bool,
    pub has_private_key: bool,
    pub sha256_fingerprint: String,
}

/// Writes `bytes` to `output` atomically: a sibling temporary file is written
/// and flushed first, then renamed over the target.
fn write_atomic(output: &str, bytes: &[u8]) -> PdfResult<()> {
    let output = Path::new(output);
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
        }
    }
    let temporary = pdfcore::docutil::temp_sibling(output);
    std::fs::write(&temporary, bytes).map_err(PdfError::from_io)?;
    if output.exists() {
        std::fs::remove_file(output).map_err(PdfError::from_io)?;
    }
    std::fs::rename(&temporary, output).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        PdfError::from_io(error)
    })
}

/// Signs a PDF with either a PKCS#12 file or a certificate from the Windows
/// personal store, writes the result atomically and returns the verification
/// report of the signature that was actually produced.
#[tauri::command]
pub async fn pdf_sign(
    input: String,
    output: String,
    pfx_path: Option<String>,
    pfx_password: Option<String>,
    cert_index: Option<usize>,
    options: SignOptionsDto,
) -> Result<SignResultDto, PdfError> {
    tauri::async_runtime::spawn_blocking(move || {
        let pdf = std::fs::read(&input).map_err(PdfError::from_io)?;

        // Certificate selection. Exactly one source must be provided.
        let (cert_der, key_der, chain) = if let Some(path) = pfx_path.as_deref() {
            let pfx = std::fs::read(path).map_err(PdfError::from_io)?;
            // The password lives only in this scope; it is never logged.
            let password = pfx_password.as_deref().unwrap_or("");
            let identity = sign::parse_pkcs12(&pfx, password)?;
            (identity.cert_der, identity.key_pkcs8_der, identity.chain_der)
        } else if let Some(index) = cert_index {
            #[cfg(windows)]
            {
                windows_store::certificate_with_private_key(index)?
            }
            #[cfg(not(windows))]
            {
                let _ = index;
                return Err(PdfError::Unsupported(
                    "the system certificate store is only available on Windows; choose a PFX file"
                        .into(),
                ));
            }
        } else {
            return Err(PdfError::InvalidInput(
                "choose a certificate (a PFX file, or a Windows store entry)".into(),
            ));
        };

        let signed = sign::sign_pdf(
            &pdf,
            &cert_der,
            &key_der,
            &chain,
            &options.into_options(),
        )?;
        write_atomic(&output, &signed)?;

        // Re-verify the file that was written and report that real result.
        let report = sign::verify_signatures(&signed);
        let expected = sign::certificate_fingerprint(&cert_der)?;
        let signature = report
            .signatures
            .into_iter().rfind(|entry| entry.signer.sha256_fingerprint == expected)
            .ok_or_else(|| {
                PdfError::ProcessingFailed(
                    "the produced signature could not be verified after writing".into(),
                )
            })?;

        Ok(SignResultDto {
            output,
            signature,
        })
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// Archives the validation data of every signature in the document.
///
/// This is the offline half of PAdES B-LT: the certificates of each chain (and
/// any revocation data the CMS carries) are written into a /DSS dictionary as an
/// incremental update, so the signed revision - and every signature in it -
/// stays byte for byte as it was. Nothing is fetched from the network; what the
/// signature already carries is what gets archived.
#[tauri::command]
pub async fn pdf_archive_validation_data(
    input: String,
    output: Option<String>,
) -> Result<pdfcore::ltv::LtvReport, PdfError> {
    tauri::async_runtime::spawn_blocking(move || {
        let pdf = std::fs::read(&input).map_err(PdfError::from_io)?;
        let (updated, report) = pdfcore::ltv::add_validation_data(&pdf)?;
        let target = match output {
            Some(path) => path,
            None => pdfcore::docutil::default_output_for(Path::new(&input), "-ltv")
                .to_string_lossy()
                .to_string(),
        };
        write_atomic(&target, &updated)?;
        Ok(report)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// Verifies every signature embedded in a PDF on disk.
#[tauri::command]
pub async fn pdf_verify_signatures(path: String) -> Result<SignatureReport, PdfError> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(PdfError::from_io)?;
        Ok(sign::verify_signatures(&bytes))
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// Lists the personal certificates of the current Windows user ("My" store).
///
/// On non-Windows platforms the list is empty: Android and other platforms
/// have no equivalent store access from Rust, so signing there always uses a
/// PFX/P12 file.
#[tauri::command]
pub async fn pdf_list_signing_certificates() -> Result<Vec<CertificateSummaryDto>, PdfError> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        {
            windows_store::list_personal_certificates()
        }
        #[cfg(not(windows))]
        {
            Ok(Vec::new())
        }
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

// ---------------------------------------------------------------------------
// Windows certificate store (Crypt32 + NCrypt)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod windows_store {
    use super::{CertificateSummaryDto, PdfError, PdfResult};
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Security::Cryptography::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Opens the CurrentUser "My" store read-only.
    unsafe fn open_personal_store() -> PdfResult<HCERTSTORE> {
        let name = wide("MY");
        CertOpenStore(
            CERT_STORE_PROV_SYSTEM_W,
            X509_ASN_ENCODING,
            None,
            CERT_OPEN_STORE_FLAGS(CERT_SYSTEM_STORE_CURRENT_USER) | CERT_STORE_READONLY_FLAG,
            Some(name.as_ptr() as *const c_void),
        )
        .map_err(|error| {
            PdfError::ProcessingFailed(format!("could not open the certificate store: {error}"))
        })
    }

    unsafe fn close_store(store: HCERTSTORE) {
        let _ = CertCloseStore(Some(store), 0);
    }

    /// Reads a display string (subject/issuer) from a certificate context.
    unsafe fn name_string(context: *const CERT_CONTEXT, kind: u32, flags: u32) -> String {
        let length = CertGetNameStringW(context, kind, flags, None, None);
        if length <= 1 {
            return String::new();
        }
        let mut buffer = vec![0u16; length as usize];
        let written = CertGetNameStringW(context, kind, flags, None, Some(&mut buffer));
        if written == 0 {
            return String::new();
        }
        let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    unsafe fn blob_hex(blob: &CRYPT_INTEGER_BLOB) -> String {
        if blob.pbData.is_null() || blob.cbData == 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize);
        let mut start = 0;
        while start + 1 < bytes.len() && bytes[start] == 0 {
            start += 1;
        }
        bytes[start..]
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect()
    }

    /// Converts a FILETIME to a UTC timestamp string.
    fn filetime_string(filetime: &FILETIME) -> String {
        let ticks = ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64;
        // 100ns intervals since 1601-01-01; convert to Unix seconds.
        let unix_seconds = ticks.saturating_sub(116_444_736_000_000_000) / 10_000_000;
        match time::OffsetDateTime::from_unix_timestamp(unix_seconds as i64) {
            Ok(stamp) => format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                stamp.year(),
                stamp.month() as u8,
                stamp.day(),
                stamp.hour(),
                stamp.minute(),
                stamp.second()
            ),
            Err(_) => String::new(),
        }
    }

    fn filetime_expired(filetime: &FILETIME) -> bool {
        let ticks = ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64;
        let unix_seconds = ticks.saturating_sub(116_444_736_000_000_000) / 10_000_000;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        unix_seconds < now
    }

    unsafe fn summarize(context: *const CERT_CONTEXT, index: usize) -> CertificateSummaryDto {
        let info = &*(*context).pCertInfo;
        let certificate_der =
            std::slice::from_raw_parts((*context).pbCertEncoded, (*context).cbCertEncoded as usize);
        let mut property_size = 0u32;
        let has_private_key = CertGetCertificateContextProperty(
            context,
            CERT_KEY_PROV_INFO_PROP_ID,
            None,
            &mut property_size,
        )
        .is_ok();
        CertificateSummaryDto {
            index,
            subject: name_string(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0),
            issuer: name_string(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, CERT_NAME_ISSUER_FLAG),
            serial_hex: blob_hex(&info.SerialNumber),
            not_before: filetime_string(&info.NotBefore),
            not_after: filetime_string(&info.NotAfter),
            expired: filetime_expired(&info.NotAfter),
            has_private_key,
            sha256_fingerprint: pdfcore::sign::certificate_fingerprint(certificate_der)
                .unwrap_or_default(),
        }
    }

    /// Lists every certificate in the CurrentUser personal store.
    pub fn list_personal_certificates() -> PdfResult<Vec<CertificateSummaryDto>> {
        unsafe {
            let store = open_personal_store()?;
            let mut certificates = Vec::new();
            let mut previous: Option<*const CERT_CONTEXT> = None;
            loop {
                let context = CertEnumCertificatesInStore(store, previous);
                if context.is_null() {
                    break;
                }
                certificates.push(summarize(context, certificates.len()));
                // Passing the context back frees it; the final NULL call frees
                // the last one as well.
                previous = Some(context);
            }
            close_store(store);
            Ok(certificates)
        }
    }

    /// Exports the PKCS#8 private key of the store certificate at `index`.
    /// Keys that Windows marks as non-exportable produce a clear error telling
    /// the user to use a PFX file - nothing is faked.
    pub fn certificate_with_private_key(index: usize) -> PdfResult<(Vec<u8>, Vec<u8>, Vec<Vec<u8>>)> {
        unsafe {
            let store = open_personal_store()?;
            let mut context: *const CERT_CONTEXT = std::ptr::null();
            let mut current_index = 0usize;
            let mut previous: Option<*const CERT_CONTEXT> = None;
            loop {
                let candidate = CertEnumCertificatesInStore(store, previous);
                if candidate.is_null() {
                    break;
                }
                if current_index == index {
                    // Keep this context (it is owned by us now); stop without
                    // passing it back so it is not freed.
                    context = candidate;
                    break;
                }
                previous = Some(candidate);
                current_index += 1;
            }
            if context.is_null() {
                close_store(store);
                return Err(PdfError::InvalidInput(
                    "the selected certificate is no longer in the store; refresh the list".into(),
                ));
            }

            let certificate_der =
                std::slice::from_raw_parts((*context).pbCertEncoded, (*context).cbCertEncoded as usize)
                    .to_vec();
            let key = export_private_key(context);
            let _ = CertFreeCertificateContext(Some(context));
            close_store(store);
            key.map(|key| (certificate_der, key, Vec::new()))
        }
    }

    /// Acquires the certificate's private key handle. CNG (KSP) keys need the
    /// `ONLY_NCRYPT` flag; legacy CSP keys need a plain silent acquire, so both
    /// are attempted before giving up.
    unsafe fn acquire_key(
        context: *const CERT_CONTEXT,
    ) -> Result<(HCRYPTPROV_OR_NCRYPT_KEY_HANDLE, CERT_KEY_SPEC, BOOL), windows::core::Error> {
        let mut last_error = None;
        for flags in [
            CRYPT_ACQUIRE_SILENT_FLAG | CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG,
            CRYPT_ACQUIRE_SILENT_FLAG,
        ] {
            let mut handle = HCRYPTPROV_OR_NCRYPT_KEY_HANDLE(0);
            let mut key_spec = CERT_KEY_SPEC(0);
            let mut caller_must_free = BOOL(0);
            match CryptAcquireCertificatePrivateKey(
                context,
                flags,
                None,
                &mut handle,
                Some(&mut key_spec),
                Some(&mut caller_must_free),
            ) {
                Ok(()) => return Ok((handle, key_spec, caller_must_free)),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(windows::core::Error::from_win32))
    }

    unsafe fn export_private_key(context: *const CERT_CONTEXT) -> PdfResult<Vec<u8>> {
        let (handle, key_spec, caller_must_free) = acquire_key(context).map_err(|error| {
            PdfError::Unsupported(format!(
                "the private key of this certificate could not be used ({error}); \
                 export the certificate as a PFX file and sign with that instead"
            ))
        })?;
        let result = if key_spec == CERT_NCRYPT_KEY_SPEC {
            export_cng(handle.0)
        } else {
            export_capi(handle.0, key_spec.0)
        };

        if caller_must_free.as_bool() {
            if key_spec == CERT_NCRYPT_KEY_SPEC {
                let _ = NCryptFreeObject(NCRYPT_HANDLE(handle.0));
            } else {
                let _ = CryptReleaseContext(handle.0, 0);
            }
        }
        result
    }

    /// CAPI provider key -> PKCS#8.
    unsafe fn export_capi(provider: usize, key_spec: u32) -> PdfResult<Vec<u8>> {        let mut length = 0u32;
        CryptExportPKCS8(
            provider,
            key_spec,
            szOID_RSA_RSA,
            0,
            None,
            None,
            &mut length,
        )
        .map_err(|error| {
            PdfError::Unsupported(format!(
                "the private key is not exportable ({error}); use a PFX file instead"
            ))
        })?;
        let mut buffer = vec![0u8; length as usize];
        CryptExportPKCS8(
            provider,
            key_spec,
            szOID_RSA_RSA,
            0,
            None,
            Some(buffer.as_mut_ptr()),
            &mut length,
        )
        .map_err(|error| {
            PdfError::Unsupported(format!(
                "the private key is not exportable ({error}); use a PFX file instead"
            ))
        })?;
        buffer.truncate(length as usize);
        Ok(buffer)
    }

    /// CNG key -> PKCS#8.
    unsafe fn export_cng(handle: usize) -> PdfResult<Vec<u8>> {
        let key = NCRYPT_KEY_HANDLE(handle);
        let mut length = 0u32;
        NCryptExportKey(
            key,
            None,
            NCRYPT_PKCS8_PRIVATE_KEY_BLOB,
            None,
            None,
            &mut length,
            NCRYPT_FLAGS(0),
        )
        .map_err(|error| {
            PdfError::Unsupported(format!(
                "the private key is not exportable ({error}); use a PFX file instead"
            ))
        })?;
        let mut buffer = vec![0u8; length as usize];
        NCryptExportKey(
            key,
            None,
            NCRYPT_PKCS8_PRIVATE_KEY_BLOB,
            None,
            Some(&mut buffer),
            &mut length,
            NCRYPT_FLAGS(0),
        )
        .map_err(|error| {
            PdfError::Unsupported(format!(
                "the private key is not exportable ({error}); use a PFX file instead"
            ))
        })?;
        buffer.truncate(length as usize);
        Ok(buffer)
    }
}

#[cfg(all(test, windows))]
mod tests {
    /// Smoke test: the Crypt32 enumeration must be callable and must not
    /// panic. An empty result is a valid environment (no personal certs).
    #[test]
    fn windows_store_listing_is_callable() {
        let list = super::windows_store::list_personal_certificates();
        match list {
            Ok(certificates) => {
                println!("{} personal certificates", certificates.len());
                for certificate in &certificates {
                    println!(
                        "#{} {} | key={} | {} .. {} | {}",
                        certificate.index,
                        certificate.subject,
                        certificate.has_private_key,
                        certificate.not_before,
                        certificate.not_after,
                        certificate.sha256_fingerprint
                    );
                    if certificate.has_private_key {
                        // Exportable keys export; non-exportable (TPM) keys
                        // must produce a clear error, never a fake signature.
                        match super::windows_store::certificate_with_private_key(certificate.index) {
                            Ok((cert_der, key, _)) => println!(
                                "    export ok: cert {} bytes, pkcs8 {} bytes",
                                cert_der.len(),
                                key.len()
                            ),
                            Err(error) => println!("    refused: {error}"),
                        }
                    }
                }
            }
            Err(error) => println!("store listing failed: {error}"),
        }
    }

    /// Full store path: exports an exportable certificate and signs a real PDF
    /// with it. The test is a no-op unless a "PDF SAK Store Test" certificate
    /// exists in the personal store (create one with
    /// `New-SelfSignedCertificate -Subject "CN=PDF SAK Store Test"
    ///  -CertStoreLocation Cert:\CurrentUser\My -KeyExportPolicy Exportable`).
    #[test]
    fn windows_store_signs_a_real_pdf() {
        let certificates = match super::windows_store::list_personal_certificates() {
            Ok(certificates) => certificates,
            Err(_) => return,
        };
        let Some(certificate) = certificates
            .iter()
            .find(|certificate| certificate.subject.contains("PDF SAK Store Test"))
        else {
            println!("no test certificate in the store; skipping");
            return;
        };
        let (cert_der, key_der, chain) =
            super::windows_store::certificate_with_private_key(certificate.index).expect("export");
        let input = std::fs::read("../samples/sample-1.pdf").expect("sample pdf");
        let signed = pdfcore::sign::sign_pdf(
            &input,
            &cert_der,
            &key_der,
            &chain,
            &pdfcore::sign::SignOptions::default(),
        )
        .expect("sign with store key");
        let report = pdfcore::sign::verify_signatures(&signed);
        let info = report.signatures.first().expect("one signature");
        assert!(info.signature_valid, "store signature must verify: {info:?}");
        assert!(info.digest_matches);
        assert!(info.covers_whole_document);
        println!("store signing ok: {} / {}", info.algorithm, info.signer.subject);
    }
}
