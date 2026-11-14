use crate::error::PdfError;
use crate::paths::OutputSpec;

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFormFieldsRequest {
    pub input: String,
    pub output: OutputSpec,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub fields: Vec<pdfcore::form_builder::NewFormField>,
}

/// Adds interactive AcroForm fields (`/Tx`, `/Btn`, `/Ch`, `/Sig`) to a PDF
/// and writes the updated document to `output`.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_create_form_fields(request: CreateFormFieldsRequest) -> Result<u32, PdfError> {
    tokio::task::spawn_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let mut document = pdfcore::docutil::load_document(input.as_path(), request.password.as_deref())?;
        let (output, policy) = request.output.resolve()?;
        let target = pdfcore::docutil::resolve_output_path(&output, policy)?;
        let count = pdfcore::form_builder::create_form_fields(&mut document, &request.fields)?;
        pdfcore::docutil::save_document(&mut document, &target, true)?;
        Ok(count as u32)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}
