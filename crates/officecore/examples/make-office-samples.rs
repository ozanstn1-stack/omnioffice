//! Generates the safe test documents used for manual and automated checks:
//!   samples/test-document.docx / .odt / .rtf
//!   samples/test-spreadsheet.xlsx / .ods / .csv
//!   samples/test-presentation.pptx / .odp
//!
//! All content is original and contains no personal data.

use officecore::csvio::{write_csv, CsvOptions};
use officecore::model::*;
use officecore::{docx, odf, pptx, rtf, xlsx};
use std::path::PathBuf;

fn sample_png() -> Vec<u8> {
    let mut image = image::RgbaImage::new(160, 90);
    for (x, _y, pixel) in image.enumerate_pixels_mut() {
        let blue = (x as f32 / 160.0 * 255.0) as u8;
        *pixel = image::Rgba([37, 99, blue, 255]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn writer_document() -> TextDocument {
    let mut document = TextDocument::new_blank("Test document");
    document.header = vec![Block::paragraph("Office Swiss Army Knife - test document")];
    document.footer = vec![Block::Paragraph {
        props: ParaProps { align: "center".into(), space_after_pt: 0.0, ..Default::default() },
        runs: vec![Run { text: "Page {{page}} of {{pages}}".into(), ..Default::default() }],
    }];
    let mut table = TableData::simple(4, 3, 460.0);
    table.rows[0].cells[0].blocks = vec![Block::paragraph("Item")];
    table.rows[0].cells[1].blocks = vec![Block::paragraph("Quantity")];
    table.rows[0].cells[2].blocks = vec![Block::paragraph("Price")];
    for (index, (name, quantity, price)) in
        [("Pen", "10", "5.50"), ("Notebook", "3", "12.00"), ("Clip", "25", "0.40")].iter().enumerate()
    {
        let row = index + 1;
        table.rows[row].cells[0].blocks = vec![Block::paragraph(name)];
        table.rows[row].cells[1].blocks = vec![Block::paragraph(quantity)];
        table.rows[row].cells[2].blocks = vec![Block::paragraph(price)];
    }
    document.blocks = vec![
        Block::heading("Test document", 1),
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "This paragraph contains ".into(), ..Default::default() },
                Run { text: "bold".into(), bold: true, ..Default::default() },
                Run { text: ", ".into(), ..Default::default() },
                Run { text: "italic".into(), italic: true, ..Default::default() },
                Run { text: " and ".into(), ..Default::default() },
                Run { text: "underlined".into(), underline: true, ..Default::default() },
                Run { text: " text.".into(), ..Default::default() },
            ],
        },
        Block::heading("Lists", 2),
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "First bullet".into(), ..Default::default() }],
        },
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "Second bullet".into(), ..Default::default() }],
        },
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "number".into(), level: 0, start: 1, marker: "1.".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "Numbered item".into(), ..Default::default() }],
        },
        Block::heading("Table", 2),
        Block::Table { table },
        Block::heading("Image", 2),
        Block::Image {
            image: ImageData::from_bytes("sample.png", &sample_png()),
            width_pt: 240.0,
            height_pt: 135.0,
            align: "center".into(),
            caption: "Generated sample image".into(),
            wrap: "inline".into(),
        },
        Block::PageBreak,
        Block::heading("Second page", 2),
        Block::paragraph("Content that lives on the second page after an explicit page break."),
    ];
    document
}

fn workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Test spreadsheet");
    workbook.sheets.clear();
    let mut data = Sheet::new("Data");
    // Header row with formatting.
    for (index, header) in ["Name", "Quantity", "Price", "Total"].iter().enumerate() {
        let address = officecore::address::format(0, index as u32);
        data.set(
            &address,
            Cell {
                value: CellValue::Text(header.to_string()),
                formula: None,
                style: CellStyle { bold: true, fill: Some("#EEF2FF".into()), ..Default::default() },
                comment: None,
                link: None,
                ..Default::default()
            },
        );
    }
    for row in 1..=100u32 {
        let name = format!("Item {row}");
        data.set(&officecore::address::format(row, 0), Cell { value: CellValue::Text(name), ..Default::default() });
        data.set(
            &officecore::address::format(row, 1),
            Cell { value: CellValue::Number((row % 17 + 1) as f64), ..Default::default() },
        );
        data.set(
            &officecore::address::format(row, 2),
            Cell {
                value: CellValue::Number(((row % 23) as f64) + 0.5),
                style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
                ..Default::default()
            },
        );
        data.set(
            &officecore::address::format(row, 3),
            Cell {
                value: CellValue::Number(0.0),
                formula: Some(format!("=B{}*C{}", row + 1, row + 1)),
                style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
                ..Default::default()
            },
        );
    }
    data.set(
        "F1",
        Cell {
            value: CellValue::Text("Summary".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    data.set("F2", Cell { value: CellValue::Text("Total".into()), ..Default::default() });
    data.set(
        "G2",
        Cell {
            value: CellValue::Number(0.0),
            formula: Some("=SUM(D2:D101)".into()),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    data.set("F3", Cell { value: CellValue::Text("Average price".into()), ..Default::default() });
    data.set(
        "G3",
        Cell {
            value: CellValue::Number(0.0),
            formula: Some("=AVERAGE(C2:C101)".into()),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    data.set("F4", Cell { value: CellValue::Text("Largest quantity".into()), ..Default::default() });
    data.set("G4", Cell { value: CellValue::Number(0.0), formula: Some("=MAX(B2:B101)".into()), ..Default::default() });
    data.set(
        "A104",
        Cell {
            value: CellValue::Text("Reorder list".into()),
            style: CellStyle { bold: true, ..Default::default() },
            ..Default::default()
        },
    );
    for row in 0..3u32 {
        let value_row = row + 1;
        let label = format!("=IF(B{}<5,\"Reorder\",\"OK\")", value_row + 1);
        data.set(
            &officecore::address::format(104 + row, 0),
            Cell { value: CellValue::Text(String::new()), formula: Some(label), ..Default::default() },
        );
    }
    data.col_widths.insert(0, 160.0);
    data.col_widths.insert(1, 90.0);
    data.col_widths.insert(2, 90.0);
    data.col_widths.insert(3, 110.0);
    data.freeze_rows = 1;
    data.merges.push(MergeRange { start: "F1:G1".into(), end: "F1:G1".into() });
    workbook.sheets.push(data);

    let mut summary = Sheet::new("Summary");
    summary.set(
        "A1",
        Cell {
            value: CellValue::Text("Sheet overview".into()),
            style: CellStyle { bold: true, size_pt: Some(14.0), ..Default::default() },
            ..Default::default()
        },
    );
    summary.set("A3", Cell { value: CellValue::Text("Data rows".into()), ..Default::default() });
    summary.set("B3", Cell { value: CellValue::Number(100.0), ..Default::default() });
    summary.set("A4", Cell { value: CellValue::Text("Cross-sheet total".into()), ..Default::default() });
    summary.set(
        "B4",
        Cell {
            value: CellValue::Number(0.0),
            formula: Some("=SUM(Data!D2:D101)".into()),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    workbook.sheets.push(summary);
    workbook
}

fn deck() -> Deck {
    let mut deck = Deck::new_blank("Test presentation");
    deck.theme = "business".into();
    let image = ImageData::from_bytes("sample.png", &sample_png());
    deck.slides = (0..5)
        .map(|index| {
            let mut slide = Slide {
                notes: format!("Speaker notes for slide {}", index + 1),
                transition: Some(if index % 2 == 0 { "fade".into() } else { "push".into() }),
                ..Default::default()
            };
            let mut title = SlideObject::new("text", 60.0, 50.0, 840.0, 90.0);
            title.text = Some(TextFrame {
                paragraphs: vec![TextParagraph {
                    text: format!("Slide {} title", index + 1),
                    level: 0,
                    bold: true,
                    size_pt: Some(32.0),
                    ..Default::default()
                }],
                ..Default::default()
            });
            let mut body = SlideObject::new("text", 70.0, 170.0, 500.0, 280.0);
            body.text = Some(TextFrame {
                paragraphs: vec![
                    TextParagraph {
                        text: "First point".into(),
                        size_pt: Some(20.0),
                        bullet: true,
                        ..Default::default()
                    },
                    TextParagraph {
                        text: "Second point".into(),
                        size_pt: Some(20.0),
                        bullet: true,
                        ..Default::default()
                    },
                    TextParagraph {
                        text: "Third point".into(),
                        size_pt: Some(20.0),
                        bullet: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            });
            let mut shape =
                SlideObject::new(if index % 2 == 0 { "rect" } else { "ellipse" }, 620.0, 190.0, 260.0, 180.0);
            shape.style =
                Some(ShapeStyle { fill: Some("#2563EB".into()), corner_radius_pt: 10.0, ..Default::default() });
            slide.objects = vec![title, body, shape];
            if index == 2 {
                let mut picture = SlideObject::new("image", 620.0, 190.0, 260.0, 150.0);
                picture.image = Some(image.clone());
                slide.objects.push(picture);
            }
            if index == 3 {
                let mut line = SlideObject::new("arrow", 120.0, 460.0, 300.0, 40.0);
                line.line = Some(LineSpec { x2: 300.0, y2: 0.0, end_arrow: true, ..Default::default() });
                line.style =
                    Some(ShapeStyle { stroke: Some("#DC2626".into()), stroke_width_pt: 3.0, ..Default::default() });
                slide.objects.push(line);
            }
            if index == 4 {
                let mut table_object = SlideObject::new("table", 70.0, 170.0, 700.0, 220.0);
                table_object.table = Some(TableData::simple(3, 3, 700.0));
                slide.objects.push(table_object);
            }
            slide
        })
        .collect();
    deck
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("samples");
    std::fs::create_dir_all(&root)?;

    let document = writer_document();
    std::fs::write(root.join("test-document.docx"), docx::write_docx(&document)?)?;
    std::fs::write(root.join("test-document.odt"), odf::write_odt(&document)?)?;
    std::fs::write(root.join("test-document.rtf"), rtf::write_rtf(&document)?)?;

    let workbook = workbook();
    std::fs::write(root.join("test-spreadsheet.xlsx"), xlsx::write_xlsx(&workbook)?)?;
    std::fs::write(root.join("test-spreadsheet.ods"), odf::write_ods(&workbook)?)?;
    std::fs::write(root.join("test-spreadsheet.csv"), write_csv(&workbook, 0, &CsvOptions::default())?)?;

    let deck = deck();
    std::fs::write(root.join("test-presentation.pptx"), pptx::write_pptx(&deck)?)?;
    std::fs::write(root.join("test-presentation.odp"), odf::write_odp(&deck)?)?;

    println!("samples written to {}", root.display());
    Ok(())
}
