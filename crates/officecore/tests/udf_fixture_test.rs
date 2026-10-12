//! Fixture-based UDF tests.
//!
//! `tests/fixtures/uyap-ornek.udf` is a committed package whose `content.xml`
//! is **hand-authored** to follow the public structure of the format (numeric
//! alignment, `R,G,B` colours, an embedded RTF fragment, an element this build
//! does not model), so a change in the writer cannot hide a change in the
//! reader. The ZIP container itself is produced by the crate's own
//! [`ZipWriter`]; regenerate the file with
//!
//! ```text
//! cargo test -p officecore --test udf_fixture_test -- --ignored --nocapture
//! ```

use officecore::model::Block;
use officecore::udf::{read_udf, write_udf};
use officecore::zip::ZipWriter;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/uyap-ornek.udf");

/// The hand-authored `content.xml` inside the fixture.
fn fixture_xml() -> String {
    // base64 of `{\rtf1\ansi\deff0 Bu bir \b ornek\b0 ihtarname ekidir.\par}`.
    let encoded = "e1xydGYxXGFuc2lcZGVmZjAgQnUgYmlyIFxiIG9ybmVrXGIwIGlodGFybmFtZSBla2lkaXIuXHBhcn0=";
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<template format_id="1.8">
  <properties>
    <pageFormat width="595" height="842" orientation="portrait" marginTop="70" marginRight="70" marginBottom="70" marginLeft="70"/>
    <documentInfo title="Örnek Dilekçe" documentType="DİLEKÇE" createdBy="UYAP Dokuman Editoru 5.x" createdDate="2026-03-14T11:42:00"/>
  </properties>
  <elements>
    <paragraph Alignment="1" LineSpacing="1.15">
      <content startOffset="0" length="12" Family="Times New Roman" Size="12" Bold="true">ÖRNEK DİLEKÇE</content>
    </paragraph>
    <paragraph Alignment="3" LineSpacing="1.5" FirstLineIndent="36" LeftIndent="0">
      <content startOffset="13" length="20" Size="12">Sayın Müdürlüğe arz ederim. </content>
      <content startOffset="33" length="10" Size="12" Italic="true">Bilgilerinize</content>
    </paragraph>
    <table Border="true" BorderColor="148,163,184">
      <row Header="true">
        <cell Colspan="2" Rowspan="1" Background="238,242,255" Align="1">
          <paragraph Alignment="1"><content Bold="true">EK LİSTE</content></paragraph>
        </cell>
      </row>
      <row>
        <cell><paragraph><content>Sıra</content></paragraph></cell>
        <cell><paragraph><content>Açıklama</content></paragraph></cell>
      </row>
    </table>
    <ozelAlan ad="davaNo">
      <content>2026/123</content>
    </ozelAlan>
    <paragraph Alignment="0">
      <content type="rtf" encoding="base64">{encoded}</content>
    </paragraph>
  </elements>
  <headers>
    <header type="default">
      <paragraph Alignment="1"><content Size="10">Sayfa {{PAGE}} / {{TOTAL}}</content></paragraph>
    </header>
  </headers>
</template>
"#
    )
}

#[test]
fn opens_the_uyap_fixture() {
    let bytes = std::fs::read(FIXTURE).expect("the fixture is committed");
    let read = read_udf(&bytes).expect("the fixture opens");
    assert_eq!(read.document.title, "Örnek Dilekçe");
    assert_eq!(read.document.page.margin_left_pt, 70.0);

    // Two paragraphs and a table, then the unknown element is preserved, not a block.
    let paragraphs: Vec<&Block> =
        read.document.blocks.iter().filter(|block| matches!(block, Block::Paragraph { .. })).collect();
    let tables = read.document.blocks.iter().filter(|block| matches!(block, Block::Table { .. })).count();
    assert_eq!(tables, 1, "the fixture table");
    assert!(paragraphs.len() >= 3, "title, body and the RTF paragraph: {}", paragraphs.len());

    // Centred bold title, justified body with an italic segment.
    match paragraphs[0] {
        Block::Paragraph { props, runs } => {
            assert_eq!(props.align, "center");
            assert!(runs[0].bold);
            assert_eq!(runs[0].text, "ÖRNEK DİLEKÇE");
        }
        _ => unreachable!(),
    }
    match paragraphs[1] {
        Block::Paragraph { props, runs } => {
            assert_eq!(props.align, "justify");
            assert_eq!(props.line_spacing, 1.5);
            assert_eq!(props.first_line_pt, 36.0);
            assert_eq!(runs.len(), 2);
            assert!(runs[1].italic);
            assert_eq!(runs[1].text, "Bilgilerinize");
        }
        _ => unreachable!(),
    }

    // The table keeps its header row, its span and its background.
    let table = read
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.rows.len(), 2);
    assert!(table.rows[0].header);
    assert_eq!(table.rows[0].cells[0].colspan, 2);
    assert_eq!(table.rows[0].cells[0].background.as_deref(), Some("#EEF2FF"));

    // The unknown element is reported and preserved verbatim.
    assert!(read.warnings.iter().any(|warning| warning.contains("ozelAlan")), "warnings: {:?}", read.warnings);
    assert_eq!(read.document.preserved_udf.len(), 1);
    assert!(read.document.preserved_udf[0].xml.contains("2026/123"));

    // The RTF fragment becomes text and is reported.
    assert!(read.warnings.iter().any(|warning| warning.contains("RTF fragment")), "warnings: {:?}", read.warnings);
    assert!(read.document.plain_text().contains("ihtarname ekidir"), "text: {}", read.document.plain_text());

    // The header is imported.
    assert!(
        read.document.header.iter().any(|block| block.plain_text().contains("Sayfa")),
        "header: {:?}",
        read.document.header
    );
}

#[test]
fn round_trips_the_uyap_fixture() {
    let bytes = std::fs::read(FIXTURE).expect("the fixture is committed");
    let first = read_udf(&bytes).expect("the fixture opens");
    let exported = write_udf(&first.document).expect("it exports");
    let second = read_udf(&exported).expect("the export opens");
    assert_eq!(second.document.title, first.document.title);
    assert_eq!(second.document.page.margin_left_pt, first.document.page.margin_left_pt);
    assert_eq!(second.document.plain_text(), first.document.plain_text());
    // The unknown element survives the round trip.
    assert_eq!(second.document.preserved_udf.len(), 1);
    assert!(second.document.preserved_udf[0].xml.contains("ozelAlan"));
    assert!(second.document.preserved_udf[0].xml.contains("2026/123"));
}

#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn write_uyap_fixture() {
    let mut zip = ZipWriter::new();
    zip.add_text("content.xml", &fixture_xml());
    std::fs::write(FIXTURE, zip.finish()).expect("write the fixture");
    println!("wrote {FIXTURE}");
}
