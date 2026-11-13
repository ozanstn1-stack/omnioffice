//! Mutation-driven round-trip fuzzing across officecore container and text
//! formats (DOCX, XLSX, PPTX, ODT, ODS, ODP, RTF). Any parsed document is
//! re-serialized and re-parsed to verify round-trip stability without panics.

use crate::{docx, odf, pptx, rtf, xlsx};

const MAX_FUZZ_BYTES: usize = 4 * 1024 * 1024;

/// Fuzz entry point for mutation-driven office round-trip stability.
pub fn fuzz_office_roundtrip(data: &[u8]) {
    if data.len() > MAX_FUZZ_BYTES {
        return;
    }
    if data.starts_with(b"{\\rtf") {
        if let Ok(read) = rtf::read_rtf(data) {
            if let Ok(bytes) = rtf::write_rtf(&read.document) {
                let _ = rtf::read_rtf(&bytes);
            }
        }
        return;
    }
    if data.starts_with(b"PK") {
        if let Ok(read) = docx::read_docx(data) {
            if let Ok(bytes) = docx::write_docx(&read.document) {
                let _ = docx::read_docx(&bytes);
            }
        }
        if let Ok(read) = xlsx::read_workbook_bytes(data) {
            if let Ok(bytes) = xlsx::write_xlsx(&read.workbook) {
                let _ = xlsx::read_workbook_bytes(&bytes);
            }
        }
        if let Ok(read) = pptx::read_pptx(data) {
            if let Ok(bytes) = pptx::write_pptx(&read.deck) {
                let _ = pptx::read_pptx(&bytes);
            }
        }
        if let Ok(read) = odf::read_odt(data) {
            if let Ok(bytes) = odf::write_odt(&read.document) {
                let _ = odf::read_odt(&bytes);
            }
        }
        if let Ok(read) = odf::read_ods(data) {
            if let Ok(bytes) = odf::write_ods(&read.workbook) {
                let _ = odf::read_ods(&bytes);
            }
        }
        let _ = odf::read_odp(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Deck, ParaProps, Run, Sheet, Slide, TextDocument, Workbook};

    #[test]
    fn fuzz_office_roundtrip_handles_valid_and_mutated_packages() {
        let mut doc = TextDocument::default();
        doc.blocks.push(Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![Run { text: "Fuzz round-trip paragraph".into(), ..Run::default() }],
        });
        let mut docx_bytes = docx::write_docx(&doc).expect("write docx");
        fuzz_office_roundtrip(&docx_bytes);
        let mid = docx_bytes.len() / 2;
        docx_bytes[mid] ^= 0xff;
        fuzz_office_roundtrip(&docx_bytes);

        let odt_bytes = odf::write_odt(&doc).expect("write odt");
        fuzz_office_roundtrip(&odt_bytes);

        let rtf_bytes = rtf::write_rtf(&doc).expect("write rtf");
        fuzz_office_roundtrip(&rtf_bytes);

        let mut wb = Workbook::default();
        wb.sheets.push(Sheet::new("Sheet1"));
        let xlsx_bytes = xlsx::write_xlsx(&wb).expect("write xlsx");
        fuzz_office_roundtrip(&xlsx_bytes);
        let ods_bytes = odf::write_ods(&wb).expect("write ods");
        fuzz_office_roundtrip(&ods_bytes);

        let mut deck = Deck::default();
        deck.slides.push(Slide::default());
        let pptx_bytes = pptx::write_pptx(&deck).expect("write pptx");
        fuzz_office_roundtrip(&pptx_bytes);

        fuzz_office_roundtrip(b"PK\x03\x04corrupt");
        fuzz_office_roundtrip(b"{\\rtf1\\ansi broken");
        fuzz_office_roundtrip(b"plain text ignored");
    }
}
