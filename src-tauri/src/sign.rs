//! Real PDF digital signature commands.
//!
//! All cryptography lives in `pdfcore::sign`; this module only bridges it to
//! the UI: it reads files, optionally unlocks a certificate from the Windows
//! certificate store, writes the result atomically and re-verifies the file it
//! produced before reporting success.
//!
//! Secrets (PFX passwords, exported private keys) are held in memory only and
//! are never logged or written to disk.

use crate::netpolicy::UrlPolicy;
use pdfcore::error::{PdfError, PdfResult};
use pdfcore::revocation::{
    FetchBudget, RevocationFetch, MAX_CRL_BYTES, MAX_OCSP_RESPONSE_BYTES, VERIFICATION_DEADLINE,
};
use pdfcore::sign::{self, SignOptions, SignatureInfo, SignatureReport};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

/// Maximum accepted RFC 3161 response body (a token is a few KB).
const MAX_TSA_RESPONSE_BYTES: usize = 1024 * 1024;
/// Time allowed for one OCSP exchange.
const OCSP_TIMEOUT: Duration = Duration::from_secs(10);
/// CRLs can be megabytes, so they get a longer (still bounded) window.
const CRL_TIMEOUT: Duration = Duration::from_secs(30);
/// Redirects followed per revocation request.
const MAX_REVOCATION_REDIRECTS: usize = 3;

/// Exchanges a signature value for an RFC 3161 timestamp token over HTTPS
/// (plain HTTP only for a loopback TSA, mirroring the WebDAV transport rule).
fn request_timestamp(url: &str, signature: &[u8]) -> PdfResult<Vec<u8>> {
    let trimmed = url.trim();
    let parsed =
        reqwest::Url::parse(trimmed).map_err(|error| PdfError::InvalidInput(format!("invalid TSA URL: {error}")))?;
    let loopback = matches!(parsed.host_str(), Some("localhost") | Some("127.0.0.1") | Some("::1"));
    match parsed.scheme() {
        "https" => {}
        "http" if loopback => {}
        other => {
            return Err(PdfError::InvalidInput(format!(
                "the TSA URL must use https:// (plain http:// is allowed only for localhost): {other}"
            )));
        }
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60))
        .user_agent("OfficeSwissArmyKnife/3.6 (rfc3161)")
        .build()
        .map_err(|error| PdfError::Internal(format!("could not build the TSA client: {error}")))?;
    let request_body = pdfcore::timestamp::build_request(signature);
    let response = client
        .post(parsed)
        .header("Content-Type", "application/timestamp-query")
        .header("Accept", "application/timestamp-reply")
        .body(request_body)
        .send()
        .map_err(|error| {
            PdfError::ProcessingFailed(format!("the timestamp authority could not be reached: {error}"))
        })?;
    if !response.status().is_success() {
        return Err(PdfError::ProcessingFailed(format!("the timestamp authority returned HTTP {}", response.status())));
    }
    if let Some(length) = response.content_length() {
        if length as usize > MAX_TSA_RESPONSE_BYTES {
            return Err(PdfError::ProcessingFailed("the timestamp response is unreasonably large".into()));
        }
    }
    let body = response
        .bytes()
        .map_err(|error| PdfError::ProcessingFailed(format!("the timestamp response could not be read: {error}")))?;
    if body.len() > MAX_TSA_RESPONSE_BYTES {
        return Err(PdfError::ProcessingFailed("the timestamp response is unreasonably large".into()));
    }
    let token = pdfcore::timestamp::parse_response(&body)?
        .ok_or_else(|| PdfError::ProcessingFailed("the timestamp authority refused the request".into()))?;
    Ok(token.token)
}

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

/// Writes `bytes` to `output` atomically: a sibling temporary file is written,
/// flushed to stable storage and renamed over the target (which replaces it
/// atomically on Windows and Unix, with no missing-file window).
fn write_atomic(output: &Path, bytes: &[u8]) -> PdfResult<()> {
    pdfcore::docutil::write_bytes_atomic(output, bytes)
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
    tsa_url: Option<String>,
) -> Result<SignResultDto, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let pdf = crate::paths::read_input_file(&input, 2u64 * 1024 * 1024 * 1024)?;

        // Certificate selection. Exactly one source must be provided.
        let (cert_der, key_der, chain) = if let Some(path) = pfx_path.as_deref() {
            let pfx = crate::paths::read_input_file(path, 2u64 * 1024 * 1024 * 1024)?;
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
                    "the system certificate store is only available on Windows; choose a PFX file".into(),
                ));
            }
        } else {
            return Err(PdfError::InvalidInput("choose a certificate (a PFX file, or a Windows store entry)".into()));
        };

        let tsa = tsa_url.unwrap_or_default().trim().to_string();
        let signed = if tsa.is_empty() {
            sign::sign_pdf(&pdf, &cert_der, &key_der, &chain, &options.into_options())?
        } else {
            // A requested timestamp is mandatory: a TSA failure fails the
            // signing instead of silently producing an untimestamped file.
            let provider = move |signature: &[u8]| request_timestamp(&tsa, signature);
            let provider_ref: &sign::TimestampProvider<'_> = &provider;
            sign::sign_pdf_with_timestamp(
                &pdf,
                &cert_der,
                &key_der,
                &chain,
                &options.into_options(),
                Some(provider_ref),
            )?
        };
        let target = crate::paths::output_file(&output)?;
        write_atomic(target.as_path(), &signed)?;

        // Re-verify the file that was written and report that real result.
        let report = sign::verify_signatures(&signed);
        let expected = sign::certificate_fingerprint(&cert_der)?;
        let signature =
            report.signatures.into_iter().rfind(|entry| entry.signer.sha256_fingerprint == expected).ok_or_else(
                || PdfError::ProcessingFailed("the produced signature could not be verified after writing".into()),
            )?;

        // Never report success on a signature that did not verify: the file was
        // written, so the honest outcome is an error the UI can surface.
        if !signature.signature_valid {
            return Err(PdfError::ProcessingFailed(
                "the produced signature failed cryptographic verification; the file was not trusted".into(),
            ));
        }
        if !signature.digest_matches {
            return Err(PdfError::ProcessingFailed(
                "the produced signature does not cover the saved document bytes; the file was not trusted".into(),
            ));
        }

        Ok(SignResultDto { output, signature })
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
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let input_path = crate::paths::input_file(&input)?;
        let pdf = crate::paths::read_input_file(&input, 2u64 * 1024 * 1024 * 1024)?;
        let (updated, report) = pdfcore::ltv::add_validation_data(&pdf)?;
        let target = match output {
            Some(path) => crate::paths::output_file(&path)?.into_path_buf(),
            None => pdfcore::docutil::default_output_for(input_path.as_path(), "-ltv"),
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
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = crate::paths::read_input_file(&path, 2u64 * 1024 * 1024 * 1024)?;
        Ok(sign::verify_signatures(&bytes))
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// True when the stored settings turn the online revocation check on.
fn online_revocation_enabled(settings: &serde_json::Value) -> bool {
    settings.get("onlineRevocationCheck").and_then(serde_json::Value::as_bool).unwrap_or(false)
}

/// OCSP and CRL URLs come from certificates inside the (untrusted) PDF. Their
/// answers are signed by the CA, so plain http:// is fine (and usual) - but
/// only http(s) to public addresses on ports 80/443 (see `netpolicy`), or a
/// crafted certificate could point the check at the user's own network.
fn revocation_url(url: &str, policy: UrlPolicy) -> PdfResult<reqwest::Url> {
    let parsed = reqwest::Url::parse(url.trim())
        .map_err(|error| PdfError::InvalidInput(format!("invalid revocation URL {url}: {error}")))?;
    policy
        .check_resolved(&parsed)
        .map_err(|reason| PdfError::InvalidInput(format!("revocation URL {url} refused: {reason}")))?;
    Ok(parsed)
}

/// Reads at most `limit` bytes; a longer body is an error, not a truncation.
fn read_limited(reader: impl Read, limit: usize) -> PdfResult<Vec<u8>> {
    let mut body = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| PdfError::ProcessingFailed(format!("the answer could not be read: {error}")))?;
    if body.len() > limit {
        return Err(PdfError::ProcessingFailed(format!("the answer is larger than {limit} bytes")));
    }
    Ok(body)
}

fn revocation_client_builder(policy: UrlPolicy) -> reqwest::blocking::ClientBuilder {
    let builder = reqwest::blocking::Client::builder()
        .connect_timeout(OCSP_TIMEOUT)
        // Every redirect hop is held to the same policy as the first request:
        // http(s) only, public addresses only, ports 80/443 only.
        .redirect(policy.redirect_policy(MAX_REVOCATION_REDIRECTS))
        .user_agent(concat!("OmniOffice/", env!("CARGO_PKG_VERSION"), " (revocation)"));
    // Names are resolved through a filter that drops non-public addresses at
    // connect time, so a host cannot change its answer after the check.
    policy.apply(builder)
}

fn revocation_client() -> PdfResult<reqwest::blocking::Client> {
    revocation_client_builder(UrlPolicy::Public)
        .build()
        .map_err(|error| PdfError::Internal(format!("could not build the revocation client: {error}")))
}

/// The time one request may take: its own timeout, but never beyond the
/// overall deadline of the verification it belongs to.
fn request_timeout(own: Duration, deadline: Instant) -> PdfResult<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(PdfError::ProcessingFailed(format!(
            "the {} second time limit for the revocation check was reached",
            VERIFICATION_DEADLINE.as_secs()
        )));
    }
    Ok(own.min(left))
}

/// One OCSP POST or CRL download for `pdfcore::revocation`.
fn fetch_revocation(
    client: &reqwest::blocking::Client,
    policy: UrlPolicy,
    deadline: Instant,
    request: RevocationFetch<'_>,
) -> PdfResult<Vec<u8>> {
    let (builder, limit) = match request {
        RevocationFetch::Ocsp { url, body } => (
            client
                .post(revocation_url(url, policy)?)
                .timeout(request_timeout(OCSP_TIMEOUT, deadline)?)
                .header("Content-Type", "application/ocsp-request")
                .header("Accept", "application/ocsp-response")
                .body(body.to_vec()),
            MAX_OCSP_RESPONSE_BYTES,
        ),
        RevocationFetch::Crl { url } => {
            (client.get(revocation_url(url, policy)?).timeout(request_timeout(CRL_TIMEOUT, deadline)?), MAX_CRL_BYTES)
        }
    };
    let response = builder
        .send()
        .map_err(|error| PdfError::ProcessingFailed(format!("the server could not be reached: {error}")))?;
    if !response.status().is_success() {
        return Err(PdfError::ProcessingFailed(format!("the server returned HTTP {}", response.status().as_u16())));
    }
    if response.content_length().is_some_and(|length| length > limit as u64) {
        return Err(PdfError::ProcessingFailed(format!("the answer is larger than {limit} bytes")));
    }
    read_limited(response, limit)
}

/// Verifies every signature and asks each signer certificate's CA whether it
/// was revoked (OCSP, then the CRL). This is the only signature command that
/// touches the network, and it re-reads the stored setting itself so a stale
/// or buggy caller cannot reach the CA while the user has the check off.
#[tauri::command]
pub async fn pdf_verify_signatures_online(app: tauri::AppHandle, path: String) -> Result<SignatureReport, PdfError> {
    let settings = crate::commands::load_settings(app)?;
    if !online_revocation_enabled(&settings) {
        return Err(PdfError::InvalidInput(
            "online revocation checking is turned off; enable it in Settings first".into(),
        ));
    }
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = crate::paths::read_input_file(&path, 2u64 * 1024 * 1024 * 1024)?;
        let client = revocation_client()?;
        // One budget for the whole file: at most MAX_FETCHES_PER_VERIFICATION
        // requests and VERIFICATION_DEADLINE seconds, however many signatures
        // and certificates the PDF carries.
        let started = Instant::now();
        let deadline = started + VERIFICATION_DEADLINE;
        let fetch = |request: RevocationFetch<'_>| fetch_revocation(&client, UrlPolicy::Public, deadline, request);
        Ok(sign::verify_signatures_online_with_budget(
            &bytes,
            &fetch,
            pdfcore::revocation::unix_now(),
            FetchBudget::standard(started),
        ))
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
    let _permit = crate::concurrency::acquire().await;
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
        .map_err(|error| PdfError::ProcessingFailed(format!("could not open the certificate store: {error}")))
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
        bytes[start..].iter().map(|byte| format!("{byte:02X}")).collect()
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
        let certificate_der = std::slice::from_raw_parts((*context).pbCertEncoded, (*context).cbCertEncoded as usize);
        let mut property_size = 0u32;
        let has_private_key =
            CertGetCertificateContextProperty(context, CERT_KEY_PROV_INFO_PROP_ID, None, &mut property_size).is_ok();
        CertificateSummaryDto {
            index,
            subject: name_string(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0),
            issuer: name_string(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, CERT_NAME_ISSUER_FLAG),
            serial_hex: blob_hex(&info.SerialNumber),
            not_before: filetime_string(&info.NotBefore),
            not_after: filetime_string(&info.NotAfter),
            expired: filetime_expired(&info.NotAfter),
            has_private_key,
            sha256_fingerprint: pdfcore::sign::certificate_fingerprint(certificate_der).unwrap_or_default(),
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
                std::slice::from_raw_parts((*context).pbCertEncoded, (*context).cbCertEncoded as usize).to_vec();
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
        for flags in [CRYPT_ACQUIRE_SILENT_FLAG | CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG, CRYPT_ACQUIRE_SILENT_FLAG] {
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
        let result =
            if key_spec == CERT_NCRYPT_KEY_SPEC { export_cng(handle.0) } else { export_capi(handle.0, key_spec.0) };

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
    unsafe fn export_capi(provider: usize, key_spec: u32) -> PdfResult<Vec<u8>> {
        let mut length = 0u32;
        CryptExportPKCS8(provider, key_spec, szOID_RSA_RSA, 0, None, None, &mut length).map_err(|error| {
            PdfError::Unsupported(format!("the private key is not exportable ({error}); use a PFX file instead"))
        })?;
        let mut buffer = vec![0u8; length as usize];
        CryptExportPKCS8(provider, key_spec, szOID_RSA_RSA, 0, None, Some(buffer.as_mut_ptr()), &mut length).map_err(
            |error| {
                PdfError::Unsupported(format!("the private key is not exportable ({error}); use a PFX file instead"))
            },
        )?;
        buffer.truncate(length as usize);
        Ok(buffer)
    }

    /// CNG key -> PKCS#8.
    unsafe fn export_cng(handle: usize) -> PdfResult<Vec<u8>> {
        let key = NCRYPT_KEY_HANDLE(handle);
        let mut length = 0u32;
        NCryptExportKey(key, None, NCRYPT_PKCS8_PRIVATE_KEY_BLOB, None, None, &mut length, NCRYPT_FLAGS(0)).map_err(
            |error| {
                PdfError::Unsupported(format!("the private key is not exportable ({error}); use a PFX file instead"))
            },
        )?;
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
            PdfError::Unsupported(format!("the private key is not exportable ({error}); use a PFX file instead"))
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
                            Ok((cert_der, key, _)) => {
                                println!("    export ok: cert {} bytes, pkcs8 {} bytes", cert_der.len(), key.len())
                            }
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
        let Some(certificate) =
            certificates.iter().find(|certificate| certificate.subject.contains("PDF SAK Store Test"))
        else {
            println!("no test certificate in the store; skipping");
            return;
        };
        let (cert_der, key_der, chain) =
            super::windows_store::certificate_with_private_key(certificate.index).expect("export");
        let input = std::fs::read("../samples/sample-1.pdf").expect("sample pdf");
        let signed =
            pdfcore::sign::sign_pdf(&input, &cert_der, &key_der, &chain, &pdfcore::sign::SignOptions::default())
                .expect("sign with store key");
        let report = pdfcore::sign::verify_signatures(&signed);
        let info = report.signatures.first().expect("one signature");
        assert!(info.signature_valid, "store signature must verify: {info:?}");
        assert!(info.digest_matches);
        assert!(info.covers_whole_document);
        println!("store signing ok: {} / {}", info.algorithm, info.signer.subject);
    }
}

#[cfg(test)]
mod revocation_tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serves one canned HTTP response per connection on a loopback port and
    /// hands back the request heads it saw. Nothing leaves the machine.
    fn serve(responses: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!("http://{}/", listener.local_addr().expect("address"));
        let handle = std::thread::spawn(move || {
            let mut heads = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
                // Drain the request body so closing does not reset the socket.
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let _ = std::io::copy(&mut (&mut stream).take(length), &mut std::io::sink());
                let _ = stream.write_all(&response);
                heads.push(head);
            }
            heads
        });
        (url, handle)
    }

    /// The production client minus the system proxy and the public-address
    /// rule, so a loopback test server can answer.
    fn client() -> reqwest::blocking::Client {
        revocation_client_builder(UrlPolicy::Unrestricted).no_proxy().build().expect("client")
    }

    /// The production client (public addresses only) minus the system proxy.
    fn strict_client() -> reqwest::blocking::Client {
        revocation_client_builder(UrlPolicy::Public).no_proxy().build().expect("client")
    }

    fn deadline() -> Instant {
        Instant::now() + VERIFICATION_DEADLINE
    }

    /// `fetch_revocation` with the loopback-friendly policy.
    fn fetch_local(client: &reqwest::blocking::Client, request: RevocationFetch<'_>) -> PdfResult<Vec<u8>> {
        fetch_revocation(client, UrlPolicy::Unrestricted, deadline(), request)
    }

    fn ok_response(body: &[u8]) -> Vec<u8> {
        let mut response =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        response.extend_from_slice(body);
        response
    }

    #[test]
    fn the_online_check_is_off_unless_the_setting_is_true() {
        assert!(!online_revocation_enabled(&serde_json::json!({})));
        assert!(!online_revocation_enabled(&serde_json::json!({ "onlineRevocationCheck": false })));
        assert!(!online_revocation_enabled(&serde_json::json!({ "onlineRevocationCheck": "true" })));
        assert!(!online_revocation_enabled(&serde_json::Value::Null));
        assert!(online_revocation_enabled(&serde_json::json!({ "onlineRevocationCheck": true })));
    }

    #[test]
    fn only_http_and_https_revocation_urls_are_used() {
        let policy = UrlPolicy::Unrestricted;
        assert!(revocation_url("http://ocsp.example.test/", policy).is_ok());
        assert!(revocation_url(" https://crl.example.test/ca.crl ", policy).is_ok());
        for url in ["ldap://ldap.example.test/cn=ca", "file:///etc/passwd", "ftp://example.test/ca.crl", "data:,x", "x"]
        {
            assert!(revocation_url(url, policy).is_err(), "{url}");
            assert!(revocation_url(url, UrlPolicy::Public).is_err(), "{url}");
        }
    }

    #[test]
    fn urls_from_a_certificate_cannot_reach_local_or_private_hosts() {
        for url in [
            "http://127.0.0.1/ocsp",
            "http://localhost/ocsp",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.1.2.3/",
            "http://192.168.0.1:80/",
            "http://[::1]/",
            "http://[::ffff:10.0.0.1]/",
            "http://8.8.8.8:8080/",
            "http://user:secret@8.8.8.8/",
        ] {
            let error = revocation_url(url, UrlPolicy::Public).expect_err(url);
            assert!(error.to_string().contains("refused"), "{url}: {error}");
        }
        assert!(revocation_url("http://8.8.8.8/ocsp", UrlPolicy::Public).is_ok());
    }

    #[test]
    fn the_production_policy_stops_a_request_before_it_leaves_the_machine() {
        // Nothing listens here and nothing is sent: the URL is refused first.
        let error = fetch_revocation(
            &strict_client(),
            UrlPolicy::Public,
            deadline(),
            RevocationFetch::Crl { url: "http://127.0.0.1:9/ca.crl" },
        )
        .expect_err("loopback is refused");
        assert!(error.to_string().contains("refused"), "{error}");
    }

    #[test]
    fn a_redirect_to_a_local_address_is_refused_on_the_hop() {
        // The first hop is the loopback test server (reached by calling the
        // client directly, bypassing the URL check); its redirect to the cloud
        // metadata address must be stopped by the production redirect policy.
        let redirect =
            b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/meta\r\nContent-Length: 0\r\n\r\n".to_vec();
        let (url, server) = serve(vec![redirect]);
        let error = strict_client().get(&url).send().expect_err("the redirect must not be followed");
        assert!(error.is_redirect(), "{error}");
        assert!(format!("{error:?}").contains("redirect refused"), "{error:?}");
        assert_eq!(server.join().expect("server").len(), 1);
    }

    #[test]
    fn a_request_is_never_given_more_time_than_the_verification_has_left() {
        let now = Instant::now();
        assert_eq!(request_timeout(OCSP_TIMEOUT, now + Duration::from_secs(300)).unwrap(), OCSP_TIMEOUT);
        let clamped = request_timeout(CRL_TIMEOUT, now + Duration::from_secs(5)).unwrap();
        assert!(clamped <= Duration::from_secs(5), "{clamped:?}");
        assert!(request_timeout(CRL_TIMEOUT, now).is_err(), "no time left means no request");
    }

    #[test]
    fn bodies_over_the_limit_are_refused_not_truncated() {
        assert_eq!(read_limited(&b"abc"[..], 3).expect("fits"), b"abc");
        assert!(read_limited(&b"abcd"[..], 3).is_err());
        assert!(read_limited(std::io::repeat(0x30), MAX_OCSP_RESPONSE_BYTES).is_err());
    }

    #[test]
    fn the_ocsp_request_is_posted_with_its_content_type() {
        let (url, server) = serve(vec![ok_response(b"answer")]);
        let body = fetch_local(&client(), RevocationFetch::Ocsp { url: &url, body: b"request" }).expect("fetch");
        assert_eq!(body, b"answer");
        let heads = server.join().expect("server");
        assert!(heads[0].starts_with("post / "), "{}", heads[0]);
        assert!(heads[0].contains("content-type: application/ocsp-request"), "{}", heads[0]);
    }

    #[test]
    fn redirects_to_other_schemes_errors_and_oversized_answers_are_refused() {
        let redirect =
            b"HTTP/1.1 302 Found\r\nLocation: ftp://example.test/ca.crl\r\nContent-Length: 0\r\n\r\n".to_vec();
        let (url, _server) = serve(vec![redirect]);
        assert!(fetch_local(&client(), RevocationFetch::Crl { url: &url }).is_err());

        let (url, _server) = serve(vec![b"HTTP/1.1 500 Oops\r\nContent-Length: 0\r\n\r\n".to_vec()]);
        let error = fetch_local(&client(), RevocationFetch::Crl { url: &url }).expect_err("500");
        assert!(error.to_string().contains("HTTP 500"), "{error}");

        // No Content-Length: the limit is enforced while reading.
        let mut endless = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        endless.extend(std::iter::repeat_n(0x30u8, MAX_OCSP_RESPONSE_BYTES + 10));
        let (url, _server) = serve(vec![endless]);
        let error = fetch_local(&client(), RevocationFetch::Ocsp { url: &url, body: b"request" }).expect_err("too big");
        assert!(error.to_string().contains("larger"), "{error}");

        // A declared length over the limit is refused before reading.
        let declared = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_CRL_BYTES + 1).into_bytes();
        let (url, _server) = serve(vec![declared]);
        let error = fetch_local(&client(), RevocationFetch::Crl { url: &url }).expect_err("declared too big");
        assert!(error.to_string().contains("larger"), "{error}");
    }
}
