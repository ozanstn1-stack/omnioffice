//! e-Fatura / e-Arşiv UBL-TR invoices.
//!
//! Turkish e-invoices are UBL 2.1 documents (`urn:oasis:names:specification:
//! ubl:schema:xsd:Invoice-2`) with the `cac`/`cbc` namespaces. GİB delivers them
//! either as a bare `.xml` or inside a `.zip`; a zip can hold several invoices.
//!
//! This module reads the fields the roadmap names — supplier and customer with
//! their VKN/TCKN, the ETTN (UUID), the lines, the VAT rates and bases, the
//! withholding (tevkifat) and the payable amount — checks them against each
//! other, renders them with a deterministic Writer template (the app's own
//! layout, never the invoice's own XSLT) and aggregates a VAT summary workbook.
//! No network and no AI is involved.

use crate::error::{OfficeError, OfficeResult};
use crate::io::{file_stem, read_bytes};
use crate::model::{
    Block, Cell, CellStyle, CellValue, DocMetadata, ParaProps, Run, Sheet, TableCell, TableData, TableRow,
    TextDocument, Workbook,
};
use crate::xml::{parse_xml, XmlNode};
use crate::zip::ZipReader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Money comparisons allow half a kuruş, which is what GİB's own validators do.
const MONEY_TOLERANCE: f64 = 0.005;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UblParty {
    pub name: String,
    /// VKN (10 digits) or TCKN (11 digits).
    pub tax_id: String,
    /// `VKN` or `TCKN` as declared by the `schemeID` attribute.
    pub tax_id_scheme: String,
    pub tax_office: String,
    pub address: String,
    pub city: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UblTaxSubtotal {
    /// VAT or withholding rate in percent.
    pub rate: f64,
    pub taxable: f64,
    pub tax: f64,
    /// `KDV`, `KDV Tevkifatı`, `ÖTV`, ...
    pub name: String,
    pub exemption_reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UblLine {
    pub id: String,
    pub name: String,
    pub quantity: f64,
    pub unit: String,
    pub unit_price: f64,
    /// Line-level discount (`cac:AllowanceCharge`, indicator false).
    pub allowance: f64,
    pub line_total: f64,
    pub vat_rate: f64,
    pub vat_amount: f64,
    pub withholding_rate: f64,
    pub withholding_amount: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UblInvoice {
    pub number: String,
    /// ETTN, the unique invoice identifier (`cbc:UUID`).
    pub ettn: String,
    pub issue_date: String,
    pub issue_time: String,
    /// `SATIS`, `IADE`, `TEVKIFAT`, ... (`cbc:InvoiceTypeCode`).
    pub type_code: String,
    /// `TEMELFATURA`, `TICARIFATURA`, `EARSIVFATURA`, ... (`cbc:ProfileID`).
    pub profile: String,
    pub currency: String,
    pub supplier: UblParty,
    pub customer: UblParty,
    pub lines: Vec<UblLine>,
    pub tax_subtotals: Vec<UblTaxSubtotal>,
    pub withholding_subtotals: Vec<UblTaxSubtotal>,
    pub line_extension_amount: f64,
    pub tax_exclusive_amount: f64,
    pub tax_inclusive_amount: f64,
    pub allowance_total: f64,
    pub tax_amount: f64,
    pub withholding_amount: f64,
    pub payable_amount: f64,
    pub notes: Vec<String>,
}

/// A field whose value disagrees with the arithmetic the invoice claims.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UblIssue {
    pub field: String,
    pub expected: f64,
    pub found: f64,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UblRead {
    pub invoice: UblInvoice,
    pub warnings: Vec<String>,
    pub issues: Vec<UblIssue>,
}

/// One dated entry of the reference VAT-rate table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VatRate {
    pub rate: f64,
    pub label: String,
    /// Inclusive start date (`YYYY-MM-DD`).
    pub since: String,
    /// Inclusive end date; empty means "still in force".
    pub until: String,
}

/// The general/standard/reduced Turkish VAT rates with the dates they changed.
///
/// This is a dated reference table, not tax advice: a rate that is not in it is
/// reported by [`validate`] instead of being silently accepted.
pub fn vat_rates() -> Vec<VatRate> {
    vec![
        VatRate { rate: 20.0, label: "Genel oran".into(), since: "2023-07-10".into(), until: String::new() },
        // A zero rate is normal: exemptions and exports are invoiced at 0%.
        VatRate { rate: 0.0, label: "Sıfır oran / istisna".into(), since: "2007-01-01".into(), until: String::new() },
        VatRate {
            rate: 18.0,
            label: "Genel oran (eski)".into(),
            since: "2007-01-01".into(),
            until: "2023-07-09".into(),
        },
        VatRate { rate: 10.0, label: "İndirimli oran".into(), since: "2023-07-10".into(), until: String::new() },
        VatRate {
            rate: 8.0,
            label: "İndirimli oran (eski)".into(),
            since: "2007-01-01".into(),
            until: "2023-07-09".into(),
        },
        VatRate { rate: 1.0, label: "Temel ihtiyaç oranı".into(), since: "2007-01-01".into(), until: String::new() },
    ]
}

/// True when `rate` is one of the reference rates in force on `date`.
fn rate_is_known(rate: f64, date: &str) -> bool {
    vat_rates().iter().any(|entry| {
        (entry.rate - rate).abs() < 0.001
            && (entry.since.is_empty() || date >= entry.since.as_str())
            && (entry.until.is_empty() || date <= entry.until.as_str())
    })
}

// ---------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------

fn text_of(node: &XmlNode) -> String {
    node.deep_text().trim().to_string()
}

fn child_text(node: &XmlNode, name: &str) -> String {
    node.child(name).map(text_of).unwrap_or_default()
}

/// Parses a UBL decimal. UBL mandates the dot, but a few older generators use a
/// comma, so both are accepted.
fn number_of(node: &XmlNode) -> Option<f64> {
    let raw = text_of(node);
    if raw.is_empty() {
        return None;
    }
    let cleaned = raw.replace(' ', "");
    if let Ok(value) = cleaned.parse::<f64>() {
        return Some(value);
    }
    cleaned.replace('.', "").replace(',', ".").parse::<f64>().ok()
}

fn child_number(node: &XmlNode, name: &str) -> f64 {
    node.child(name).and_then(number_of).unwrap_or(0.0)
}

fn parse_party(node: &XmlNode) -> UblParty {
    let mut party = UblParty::default();
    // cac:PartyIdentification/cbc:ID with schemeID="VKN" or "TCKN".
    for identification in node.children_of("PartyIdentification") {
        let Some(id) = identification.child("ID") else { continue };
        let scheme = id.attr_any_ns("schemeID").unwrap_or("").to_ascii_uppercase();
        if scheme == "VKN" || scheme == "TCKN" || party.tax_id.is_empty() {
            party.tax_id = text_of(id);
            party.tax_id_scheme = scheme;
        }
    }
    if let Some(name) = node.child("PartyName").and_then(|party_name| party_name.child("Name")) {
        party.name = text_of(name);
    }
    if party.name.is_empty() {
        // Some suppliers only fill the person name.
        if let Some(person) = node.child("Person") {
            party.name = child_text(person, "FirstName");
            let family = child_text(person, "FamilyName");
            if !family.is_empty() {
                party.name = format!("{} {}", party.name, family).trim().to_string();
            }
        }
    }
    if let Some(address) = node.child("PostalAddress") {
        let street = child_text(address, "StreetName");
        let building = child_text(address, "BuildingNumber");
        let district = child_text(address, "CitySubdivisionName");
        party.city = child_text(address, "CityName");
        party.address = [street, building, district, party.city.clone()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
    }
    if let Some(tax_scheme) = node.child("PartyTaxScheme") {
        // TaxScheme/Name is the tax office in Turkish invoices.
        party.tax_office = child_text(tax_scheme, "TaxScheme");
        if party.tax_office.is_empty() {
            party.tax_office = child_text(tax_scheme, "Name");
        }
    }
    party
}

fn parse_tax_subtotals(node: &XmlNode) -> Vec<UblTaxSubtotal> {
    let mut out = Vec::new();
    for subtotal in node.children_of("TaxSubtotal") {
        let mut exemption_reason = child_text(subtotal, "TaxExemptionReasonCode");
        let name = subtotal
            .child("TaxCategory")
            .map(|category| {
                if exemption_reason.is_empty() {
                    exemption_reason = child_text(category, "TaxExemptionReasonCode");
                }
                category.child("TaxScheme").map(|scheme| child_text(scheme, "Name")).unwrap_or_default()
            })
            .unwrap_or_default();
        out.push(UblTaxSubtotal {
            taxable: child_number(subtotal, "TaxableAmount"),
            tax: child_number(subtotal, "TaxAmount"),
            rate: child_number(subtotal, "Percent"),
            name,
            exemption_reason,
        });
    }
    out
}

/// The line's discounts: `cac:AllowanceCharge` entries whose
/// `cbc:ChargeIndicator` is not true (true would be a surcharge).
fn line_allowance(node: &XmlNode) -> f64 {
    node.children_of("AllowanceCharge")
        .iter()
        .filter(|charge| !child_text(charge, "ChargeIndicator").eq_ignore_ascii_case("true"))
        .map(|charge| child_number(charge, "Amount"))
        .sum()
}

fn parse_line(node: &XmlNode) -> UblLine {
    let (quantity, unit) = node
        .child("InvoicedQuantity")
        .map(|quantity| {
            (number_of(quantity).unwrap_or(0.0), quantity.attr_any_ns("unitCode").unwrap_or("").to_string())
        })
        .unwrap_or_default();
    let name = node.child("Item").map(|item| child_text(item, "Name")).unwrap_or_default();
    let unit_price = node.child("Price").map(|price| child_number(price, "PriceAmount")).unwrap_or(0.0);
    // The line's own TaxTotal carries the rate that applies to it.
    let (vat_rate, vat_amount) = node
        .child("TaxTotal")
        .map(|tax_total| {
            let subtotals = parse_tax_subtotals(tax_total);
            match subtotals.first() {
                Some(first) => (first.rate, first.tax),
                None => (0.0, child_number(tax_total, "TaxAmount")),
            }
        })
        .unwrap_or_default();
    let (withholding_rate, withholding_amount) = node
        .child("WithholdingTaxTotal")
        .map(|withholding| {
            let amount = child_number(withholding, "TaxAmount");
            let subtotals = parse_tax_subtotals(withholding);
            let rate = subtotals.first().map(|first| first.rate).unwrap_or(0.0);
            let amount = if amount == 0.0 { subtotals.first().map(|first| first.tax).unwrap_or(0.0) } else { amount };
            (rate, amount)
        })
        .unwrap_or_default();
    UblLine {
        id: child_text(node, "ID"),
        name,
        quantity,
        unit,
        unit_price,
        allowance: line_allowance(node),
        line_total: child_number(node, "LineExtensionAmount"),
        vat_rate,
        vat_amount,
        withholding_rate,
        withholding_amount,
    }
}

/// Parses one UBL invoice document.
pub fn parse_invoice(xml: &str) -> OfficeResult<UblInvoice> {
    let root = parse_xml(xml)?;
    let invoice_node =
        root.children.iter().find(|child| matches!(child.local_name(), "Invoice" | "CreditNote")).unwrap_or(&root);
    if invoice_node.local_name() != "Invoice" && invoice_node.local_name() != "CreditNote" {
        return Err(OfficeError::corrupt("The XML is not a UBL invoice."));
    }
    let supplier = invoice_node
        .child("AccountingSupplierParty")
        .and_then(|node| node.child("Party"))
        .map(parse_party)
        .unwrap_or_default();
    let customer = invoice_node
        .child("AccountingCustomerParty")
        .and_then(|node| node.child("Party"))
        .map(parse_party)
        .unwrap_or_default();
    let (tax_amount, tax_subtotals) = invoice_node
        .child("TaxTotal")
        .map(|tax_total| (child_number(tax_total, "TaxAmount"), parse_tax_subtotals(tax_total)))
        .unwrap_or_default();
    let (withholding_amount, withholding_subtotals) = invoice_node
        .child("WithholdingTaxTotal")
        .map(|withholding| (child_number(withholding, "TaxAmount"), parse_tax_subtotals(withholding)))
        .unwrap_or_default();
    let total = invoice_node.child("LegalMonetaryTotal");
    let total_number = |name: &str| total.map(|node| child_number(node, name)).unwrap_or(0.0);
    Ok(UblInvoice {
        number: child_text(invoice_node, "ID"),
        ettn: child_text(invoice_node, "UUID"),
        issue_date: child_text(invoice_node, "IssueDate"),
        issue_time: child_text(invoice_node, "IssueTime"),
        type_code: child_text(invoice_node, "InvoiceTypeCode"),
        profile: child_text(invoice_node, "ProfileID"),
        currency: child_text(invoice_node, "DocumentCurrencyCode"),
        supplier,
        customer,
        lines: invoice_node.children_of("InvoiceLine").iter().map(|node| parse_line(node)).collect(),
        tax_subtotals,
        withholding_subtotals,
        line_extension_amount: total_number("LineExtensionAmount"),
        tax_exclusive_amount: total_number("TaxExclusiveAmount"),
        tax_inclusive_amount: total_number("TaxInclusiveAmount"),
        allowance_total: total_number("AllowanceTotalAmount"),
        tax_amount,
        withholding_amount,
        payable_amount: total_number("PayableAmount"),
        notes: invoice_node
            .children_of("Note")
            .iter()
            .map(|node| text_of(node))
            .filter(|note| !note.is_empty())
            .collect(),
    })
}

/// Checks the invoice against its own arithmetic and the dated rate table.
pub fn validate(invoice: &UblInvoice) -> Vec<UblIssue> {
    fn check(issues: &mut Vec<UblIssue>, field: &str, expected: f64, found: f64, message: String) {
        if (expected - found).abs() > MONEY_TOLERANCE {
            issues.push(UblIssue { field: field.to_string(), expected, found, message });
        }
    }
    let mut issues = Vec::new();
    let mut line_sum = 0.0;
    let mut line_vat = 0.0;
    for line in &invoice.lines {
        line_sum += line.line_total;
        line_vat += line.vat_amount;
        let expected_total = line.quantity * line.unit_price - line.allowance;
        check(
            &mut issues,
            &format!("line:{}.total", line.id),
            expected_total,
            line.line_total,
            format!("Satır {} toplamı miktar × birim fiyat − iskonto'ya eşit değil.", line.id),
        );
        let expected_vat = line.line_total * line.vat_rate / 100.0;
        check(
            &mut issues,
            &format!("line:{}.vat", line.id),
            expected_vat,
            line.vat_amount,
            format!("Satır {} KDV'si matrah × orana eşit değil.", line.id),
        );
        if !rate_is_known(line.vat_rate, &invoice.issue_date) {
            issues.push(UblIssue {
                field: format!("line:{}.rate", line.id),
                expected: line.vat_rate,
                found: line.vat_rate,
                message: format!(
                    "Satır {} için %{} oranı {} tarihli tabloda yok.",
                    line.id, line.vat_rate, invoice.issue_date
                ),
            });
        }
    }
    if !invoice.lines.is_empty() {
        check(
            &mut issues,
            "lineExtensionAmount",
            line_sum,
            invoice.line_extension_amount,
            "Satır toplamları ile mal/hizmet toplam tutarı uyuşmuyor.".into(),
        );
    }
    // The document TaxTotal can also carry ÖTV, so compare the line VAT with
    // the KDV subtotals when they exist instead of the whole tax total (which
    // made every ÖTV invoice look wrong).
    let is_kdv = |name: &str| {
        let upper = name.to_uppercase();
        upper.contains("KDV") && !upper.contains("TEVKIFAT")
    };
    let has_kdv_subtotal = invoice.tax_subtotals.iter().any(|subtotal| is_kdv(&subtotal.name));
    let kdv_total: f64 =
        invoice.tax_subtotals.iter().filter(|subtotal| is_kdv(&subtotal.name)).map(|subtotal| subtotal.tax).sum();
    let expected_tax = if has_kdv_subtotal { kdv_total } else { invoice.tax_amount };
    check(&mut issues, "taxAmount", line_vat, expected_tax, "Satır KDV'leri ile toplam KDV uyuşmuyor.".into());
    if !invoice.tax_subtotals.is_empty() {
        // The document TaxTotal's amount must equal its own subtotals (KDV +
        // ÖTV + ...), which catches a TaxAmount that disagrees with its parts.
        let subtotal_sum: f64 = invoice.tax_subtotals.iter().map(|subtotal| subtotal.tax).sum();
        check(
            &mut issues,
            "taxAmount",
            subtotal_sum,
            invoice.tax_amount,
            "Vergi toplamı, alt toplamların toplamına eşit değil.".into(),
        );
    }
    check(
        &mut issues,
        "taxExclusiveAmount",
        invoice.line_extension_amount,
        invoice.tax_exclusive_amount,
        "Vergi hariç tutar mal/hizmet toplamına eşit değil.".into(),
    );
    check(
        &mut issues,
        "taxInclusiveAmount",
        invoice.tax_exclusive_amount + invoice.tax_amount,
        invoice.tax_inclusive_amount,
        "Vergi dahil tutar, vergi hariç tutar + KDV'ye eşit değil.".into(),
    );
    check(
        &mut issues,
        "payableAmount",
        invoice.tax_inclusive_amount - invoice.withholding_amount,
        invoice.payable_amount,
        "Ödenecek tutar, vergi dahil tutar − tevkifata eşit değil.".into(),
    );
    issues
}

// ---------------------------------------------------------------------------
// Reading a package
// ---------------------------------------------------------------------------

fn looks_like_invoice(xml: &str) -> bool {
    xml.contains("Invoice-2") || xml.contains("<Invoice") || xml.contains(":Invoice")
}

/// Reads every invoice in a `.xml` or `.zip` package.
pub fn read_ubl_package(bytes: &[u8]) -> OfficeResult<Vec<UblRead>> {
    let mut warnings = Vec::new();
    let mut documents: Vec<String> = Vec::new();
    if bytes.starts_with(b"PK") {
        let reader = ZipReader::open(bytes.to_vec())?;
        let mut names: Vec<String> =
            reader.names().filter(|name| name.to_ascii_lowercase().ends_with(".xml")).map(str::to_string).collect();
        // Prefer entries that look like invoices.
        names.sort_by_key(|name| !name.to_ascii_lowercase().contains("invoice"));
        for name in names {
            match reader.read_text(&name) {
                Ok(xml) if looks_like_invoice(&xml) => documents.push(xml),
                Ok(_) => warnings.push(format!("{name} is not a UBL invoice and was skipped.")),
                Err(error) => warnings.push(format!("{name} could not be read: {error}")),
            }
        }
        if documents.is_empty() {
            return Err(OfficeError::corrupt("The package holds no UBL invoice."));
        }
    } else {
        let xml = crate::zip::decode_utf8(bytes, "invoice")?;
        if !looks_like_invoice(&xml) {
            return Err(OfficeError::corrupt("The XML is not a UBL invoice."));
        }
        documents.push(xml);
    }
    let mut out = Vec::new();
    for xml in documents {
        let invoice = parse_invoice(&xml)?;
        let issues = validate(&invoice);
        let mut read_warnings = warnings.clone();
        if invoice.supplier.tax_id_scheme == "TCKN" {
            read_warnings.push("Satıcı VKN yerine TCKN ile tanımlanmış.".into());
        }
        out.push(UblRead { invoice, warnings: read_warnings, issues });
    }
    Ok(out)
}

/// Reads the first invoice of a package.
pub fn read_ubl(bytes: &[u8]) -> OfficeResult<UblRead> {
    read_ubl_package(bytes)?.into_iter().next().ok_or_else(|| OfficeError::corrupt("The package holds no UBL invoice."))
}

pub fn read_ubl_file(path: &Path) -> OfficeResult<UblRead> {
    read_ubl(&read_bytes(path)?)
}

pub fn read_ubl_package_file(path: &Path) -> OfficeResult<Vec<UblRead>> {
    read_ubl_package(&read_bytes(path)?)
}

// ---------------------------------------------------------------------------
// Deterministic Writer template
// ---------------------------------------------------------------------------

fn paragraph(text: &str, style: &str, bold: bool) -> Block {
    Block::Paragraph {
        props: ParaProps { style: style.into(), ..Default::default() },
        runs: vec![Run { text: text.to_string(), bold, ..Default::default() }],
    }
}

fn money(value: f64) -> String {
    // Turkish display: thousands with a dot, decimals with a comma.
    let formatted = format!("{value:.2}");
    let (whole, cents) = formatted.split_once('.').unwrap_or((formatted.as_str(), "00"));
    let mut grouped = String::new();
    let digits: Vec<char> = whole.chars().collect();
    for (index, ch) in digits.iter().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(*ch);
    }
    format!("{grouped},{cents}")
}

fn table(rows: Vec<Vec<String>>, widths: Vec<f64>, header: bool) -> Block {
    let table_rows = rows
        .iter()
        .enumerate()
        .map(|(index, cells)| TableRow {
            cells: cells
                .iter()
                .map(|text| TableCell { blocks: vec![Block::paragraph(text)], ..Default::default() })
                .collect(),
            height_pt: None,
            header: header && index == 0,
        })
        .collect();
    Block::Table {
        table: TableData {
            rows: table_rows,
            column_widths_pt: widths,
            borders: true,
            border_color: "#94A3B8".into(),
            align: "left".into(),
        },
    }
}

/// Renders the invoice with the app's own deterministic layout (never the
/// invoice's XSLT).
pub fn invoice_to_document(invoice: &UblInvoice) -> TextDocument {
    let mut document = TextDocument::new_blank(&invoice.number);
    let currency = if invoice.currency.is_empty() { "TRY".to_string() } else { invoice.currency.clone() };
    let mut blocks = vec![
        paragraph("e-FATURA / e-ARŞİV", "Title", true),
        paragraph(
            &format!(
                "Fatura No: {} · Tarih: {} {}",
                if invoice.number.is_empty() { "-" } else { &invoice.number },
                invoice.issue_date,
                invoice.issue_time
            ),
            "Subtitle",
            false,
        ),
    ];
    if !invoice.ettn.is_empty() {
        blocks.push(paragraph(&format!("ETTN: {}", invoice.ettn), "Normal", false));
    }
    if !invoice.profile.is_empty() || !invoice.type_code.is_empty() {
        blocks.push(paragraph(&format!("Senaryo: {} · Tür: {}", invoice.profile, invoice.type_code), "Normal", false));
    }
    let party = |label: &str, party: &UblParty| -> Vec<String> {
        let id = if party.tax_id.is_empty() {
            String::new()
        } else {
            format!(
                "{}: {}",
                if party.tax_id_scheme.is_empty() { "VKN/TCKN" } else { &party.tax_id_scheme },
                party.tax_id
            )
        };
        vec![
            format!("{label}: {}", party.name),
            id,
            if party.tax_office.is_empty() { String::new() } else { format!("Vergi Dairesi: {}", party.tax_office) },
            party.address.clone(),
        ]
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect()
    };
    blocks.push(table(
        vec![
            vec!["SATICI".into(), "ALICI".into()],
            vec![party("", &invoice.supplier).join("\n"), party("", &invoice.customer).join("\n")],
        ],
        vec![230.0, 230.0],
        true,
    ));
    let mut line_rows = vec![vec![
        "Sıra".into(),
        "Mal / Hizmet".into(),
        "Miktar".into(),
        "Birim Fiyat".into(),
        "Tutar".into(),
        "KDV %".into(),
        "KDV".into(),
    ]];
    for (index, line) in invoice.lines.iter().enumerate() {
        let id = if line.id.is_empty() { (index + 1).to_string() } else { line.id.clone() };
        line_rows.push(vec![
            id,
            line.name.clone(),
            format!("{} {}", money(line.quantity), line.unit),
            money(line.unit_price),
            money(line.line_total),
            format!("%{}", money(line.vat_rate)),
            money(line.vat_amount),
        ]);
    }
    blocks.push(table(line_rows, vec![30.0, 150.0, 55.0, 65.0, 65.0, 40.0, 55.0], true));
    blocks.push(paragraph(
        &format!(
            "Mal/Hizmet Toplam Tutarı: {} {currency}\nHesaplanan KDV: {} {currency}\nTevkifat: {} {currency}\nÖdenecek Tutar: {} {currency}",
            money(invoice.line_extension_amount),
            money(invoice.tax_amount),
            money(invoice.withholding_amount),
            money(invoice.payable_amount),
        ),
        "Normal",
        true,
    ));
    if !invoice.tax_subtotals.is_empty() {
        let mut rows = vec![vec!["KDV Oranı".into(), "Matrah".into(), "KDV".into()]];
        for subtotal in &invoice.tax_subtotals {
            rows.push(vec![format!("%{}", money(subtotal.rate)), money(subtotal.taxable), money(subtotal.tax)]);
        }
        blocks.push(paragraph("KDV Özeti", "Heading3", true));
        blocks.push(table(rows, vec![100.0, 130.0, 130.0], true));
    }
    if !invoice.withholding_subtotals.is_empty() {
        let mut rows = vec![vec!["Tevkifat Oranı".into(), "Matrah".into(), "Tevkifat".into()]];
        for subtotal in &invoice.withholding_subtotals {
            rows.push(vec![format!("%{}", money(subtotal.rate)), money(subtotal.taxable), money(subtotal.tax)]);
        }
        blocks.push(paragraph("Tevkifat", "Heading3", true));
        blocks.push(table(rows, vec![100.0, 130.0, 130.0], true));
    }
    for note in &invoice.notes {
        blocks.push(paragraph(note, "Caption", false));
    }
    document.blocks = blocks;
    document.metadata = DocMetadata {
        title: invoice.number.clone(),
        author: invoice.supplier.name.clone(),
        subject: format!("e-Fatura {}", invoice.ettn),
        ..Default::default()
    };
    document
}

// ---------------------------------------------------------------------------
// VAT summary workbook
// ---------------------------------------------------------------------------

/// The month (`YYYY-MM`) an invoice belongs to, for the summary grouping.
pub fn invoice_month(invoice: &UblInvoice) -> String {
    if invoice.issue_date.len() >= 7 {
        invoice.issue_date[..7].to_string()
    } else {
        invoice.issue_date.clone()
    }
}

fn text_cell(value: &str, style: CellStyle) -> Cell {
    Cell {
        value: if value.is_empty() { CellValue::Empty } else { CellValue::Text(value.to_string()) },
        style,
        ..Default::default()
    }
}

fn number_cell(value: f64, style: CellStyle) -> Cell {
    Cell { value: CellValue::Number(value), style, ..Default::default() }
}

/// Aggregates a VAT summary by month, supplier and rate, and adds a totals row.
pub fn vat_summary_workbook(invoices: &[UblInvoice]) -> Workbook {
    // month -> supplier -> rate -> (base, vat)
    let mut groups: BTreeMap<(String, String, String), (f64, f64)> = BTreeMap::new();
    for invoice in invoices {
        let month = invoice_month(invoice);
        let supplier = if invoice.supplier.name.is_empty() {
            invoice.supplier.tax_id.clone()
        } else {
            invoice.supplier.name.clone()
        };
        // Use the document-level subtotals; fall back to the lines.
        if invoice.tax_subtotals.is_empty() {
            for line in &invoice.lines {
                let key = (month.clone(), supplier.clone(), format!("{:.2}", line.vat_rate));
                let entry = groups.entry(key).or_insert((0.0, 0.0));
                entry.0 += line.line_total;
                entry.1 += line.vat_amount;
            }
        } else {
            for subtotal in &invoice.tax_subtotals {
                let key = (month.clone(), supplier.clone(), format!("{:.2}", subtotal.rate));
                let entry = groups.entry(key).or_insert((0.0, 0.0));
                entry.0 += subtotal.taxable;
                entry.1 += subtotal.tax;
            }
        }
    }
    let header = CellStyle { bold: true, fill: Some("#EEF2FF".into()), ..Default::default() };
    let money_style = CellStyle { number_format: "#,##0.00".into(), ..Default::default() };
    let bold_money = CellStyle { bold: true, number_format: "#,##0.00".into(), ..Default::default() };
    let mut sheet = Sheet::new("KDV Özeti");
    let headers = ["Ay", "Satıcı", "VKN/TCKN", "Oran (%)", "Matrah", "KDV", "Tevkifat", "Ödenecek"];
    for (index, title) in headers.iter().enumerate() {
        let column = String::from_utf8(vec![b'A' + index as u8]).unwrap();
        sheet.set(&format!("{column}1"), text_cell(title, header.clone()));
    }
    // Supplier tax id per name, for the VKN column.
    let mut tax_ids: BTreeMap<String, String> = BTreeMap::new();
    for invoice in invoices {
        let supplier = if invoice.supplier.name.is_empty() {
            invoice.supplier.tax_id.clone()
        } else {
            invoice.supplier.name.clone()
        };
        tax_ids.entry(supplier).or_insert_with(|| invoice.supplier.tax_id.clone());
    }
    let mut row = 2u32;
    let mut base_total = 0.0;
    let mut vat_total = 0.0;
    for ((month, supplier, rate), (base, vat)) in &groups {
        sheet.set(&format!("A{row}"), text_cell(month, CellStyle::default()));
        sheet.set(&format!("B{row}"), text_cell(supplier, CellStyle::default()));
        sheet.set(
            &format!("C{row}"),
            text_cell(tax_ids.get(supplier).map(String::as_str).unwrap_or(""), CellStyle::default()),
        );
        sheet.set(&format!("D{row}"), number_cell(rate.parse().unwrap_or(0.0), CellStyle::default()));
        sheet.set(&format!("E{row}"), number_cell(*base, money_style.clone()));
        sheet.set(&format!("F{row}"), number_cell(*vat, money_style.clone()));
        base_total += base;
        vat_total += vat;
        row += 1;
    }
    // Withholding and payable are per-invoice, so they are summed separately.
    let withholding_total: f64 = invoices.iter().map(|invoice| invoice.withholding_amount).sum();
    let payable_total: f64 = invoices.iter().map(|invoice| invoice.payable_amount).sum();
    sheet.set(&format!("A{row}"), text_cell("TOPLAM", CellStyle { bold: true, ..Default::default() }));
    sheet.set(&format!("E{row}"), number_cell(base_total, bold_money.clone()));
    sheet.set(&format!("F{row}"), number_cell(vat_total, bold_money.clone()));
    sheet.set(&format!("G{row}"), number_cell(withholding_total, bold_money.clone()));
    sheet.set(&format!("H{row}"), number_cell(payable_total, bold_money));
    sheet.freeze_rows = 1;
    sheet.col_widths =
        BTreeMap::from([(0, 90.0), (1, 220.0), (2, 120.0), (3, 80.0), (4, 110.0), (5, 110.0), (6, 110.0), (7, 110.0)]);
    let mut workbook = Workbook::new_blank("KDV Özeti");
    workbook.sheets = vec![sheet];
    workbook.active_sheet = 0;
    workbook
}

/// Convenience: the file name an export should use for a package.
pub fn suggested_summary_title(path: &Path) -> String {
    format!("KDV Özeti - {}", file_stem(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-line invoice at 20%, matching arithmetic.
    fn invoice_20() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2"
  xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2"
  xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:UBLVersionID>2.1</cbc:UBLVersionID>
  <cbc:ID>GIB2026000000001</cbc:ID>
  <cbc:UUID>7f4e1c2a-0000-4000-8000-000000000001</cbc:UUID>
  <cbc:IssueDate>2026-03-14</cbc:IssueDate>
  <cbc:IssueTime>11:42:00</cbc:IssueTime>
  <cbc:InvoiceTypeCode>SATIS</cbc:InvoiceTypeCode>
  <cbc:ProfileID>TICARIFATURA</cbc:ProfileID>
  <cbc:DocumentCurrencyCode>TRY</cbc:DocumentCurrencyCode>
  <cac:AccountingSupplierParty><cac:Party>
    <cac:PartyIdentification><cbc:ID schemeID="VKN">1234567890</cbc:ID></cac:PartyIdentification>
    <cac:PartyName><cbc:Name>Örnek Danışmanlık A.Ş.</cbc:Name></cac:PartyName>
    <cac:PostalAddress><cbc:StreetName>Atatürk Cad.</cbc:StreetName><cbc:CitySubdivisionName>Kadıköy</cbc:CitySubdivisionName><cbc:CityName>İstanbul</cbc:CityName></cac:PostalAddress>
    <cac:PartyTaxScheme><cac:TaxScheme><cbc:Name>Kadıköy</cbc:Name></cac:TaxScheme></cac:PartyTaxScheme>
  </cac:Party></cac:AccountingSupplierParty>
  <cac:AccountingCustomerParty><cac:Party>
    <cac:PartyIdentification><cbc:ID schemeID="VKN">9876543210</cbc:ID></cac:PartyIdentification>
    <cac:PartyName><cbc:Name>Alıcı Ltd. Şti.</cbc:Name></cac:PartyName>
  </cac:Party></cac:AccountingCustomerParty>
  <cac:TaxTotal>
    <cbc:TaxAmount currencyID="TRY">300.00</cbc:TaxAmount>
    <cac:TaxSubtotal><cbc:TaxableAmount currencyID="TRY">1500.00</cbc:TaxableAmount><cbc:TaxAmount currencyID="TRY">300.00</cbc:TaxAmount><cbc:Percent>20.00</cbc:Percent>
      <cac:TaxCategory><cac:TaxScheme><cbc:Name>KDV</cbc:Name></cac:TaxScheme></cac:TaxCategory></cac:TaxSubtotal>
  </cac:TaxTotal>
  <cac:LegalMonetaryTotal>
    <cbc:LineExtensionAmount currencyID="TRY">1500.00</cbc:LineExtensionAmount>
    <cbc:TaxExclusiveAmount currencyID="TRY">1500.00</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount currencyID="TRY">1800.00</cbc:TaxInclusiveAmount>
    <cbc:PayableAmount currencyID="TRY">1800.00</cbc:PayableAmount>
  </cac:LegalMonetaryTotal>
  <cac:InvoiceLine>
    <cbc:ID>1</cbc:ID>
    <cbc:InvoicedQuantity unitCode="C62">10</cbc:InvoicedQuantity>
    <cbc:LineExtensionAmount currencyID="TRY">1500.00</cbc:LineExtensionAmount>
    <cac:TaxTotal><cbc:TaxAmount currencyID="TRY">300.00</cbc:TaxAmount>
      <cac:TaxSubtotal><cbc:TaxableAmount>1500.00</cbc:TaxableAmount><cbc:TaxAmount>300.00</cbc:TaxAmount><cbc:Percent>20.00</cbc:Percent></cac:TaxSubtotal>
    </cac:TaxTotal>
    <cac:Item><cbc:Name>Danışmanlık hizmeti</cbc:Name></cac:Item>
    <cac:Price><cbc:PriceAmount currencyID="TRY">150.00</cbc:PriceAmount></cac:Price>
  </cac:InvoiceLine>
</Invoice>"#
            .to_string()
    }

    #[test]
    fn a_zero_rate_line_is_not_reported_as_unknown() {
        // Exempt/zero-rated lines are normal; the reference table had no 0%.
        let xml = invoice_20()
            .replace("<cbc:Percent>20.00</cbc:Percent>", "<cbc:Percent>0.00</cbc:Percent>")
            .replace(">300.00<", ">0.00<")
            .replace(">1800.00<", ">1500.00<");
        let read = read_ubl(xml.as_bytes()).unwrap();
        assert!(!read.issues.iter().any(|issue| issue.field.contains(".rate")), "issues: {:?}", read.issues);
    }

    #[test]
    fn a_line_discount_is_not_reported_as_a_mismatch() {
        // 10 x 150 - 100 discount = 1400; the line total check used to ignore it.
        let xml = invoice_20()
            .replace(">1500.00<", ">1400.00<")
            .replace(">1800.00<", ">1680.00<")
            .replace(
                "<cbc:TaxAmount currencyID=\"TRY\">300.00</cbc:TaxAmount>",
                "<cbc:TaxAmount currencyID=\"TRY\">280.00</cbc:TaxAmount>",
            )
            .replace("<cbc:TaxAmount>300.00</cbc:TaxAmount>", "<cbc:TaxAmount>280.00</cbc:TaxAmount>")
            .replace(
                "<cac:Item>",
                "<cac:AllowanceCharge><cbc:ChargeIndicator>false</cbc:ChargeIndicator><cbc:Amount currencyID=\"TRY\">100.00</cbc:Amount></cac:AllowanceCharge><cac:Item>",
            );
        let read = read_ubl(xml.as_bytes()).unwrap();
        assert!(!read.issues.iter().any(|issue| issue.field.ends_with(".total")), "issues: {:?}", read.issues);
    }

    #[test]
    fn an_excise_tax_subtotal_does_not_break_the_vat_check() {
        // The document TaxTotal carries KDV (300) + ÖTV (200); the line VAT must
        // be compared with the KDV subtotal, not with the 500 total.
        let xml = invoice_20()
            .replace(
                "<cac:TaxTotal>\n    <cbc:TaxAmount currencyID=\"TRY\">300.00</cbc:TaxAmount>",
                "<cac:TaxTotal>\n    <cbc:TaxAmount currencyID=\"TRY\">500.00</cbc:TaxAmount>",
            )
            .replace(">1800.00<", ">2000.00<")
            .replace(
                "<cbc:Name>KDV</cbc:Name></cac:TaxScheme></cac:TaxCategory></cac:TaxSubtotal>",
                "<cbc:Name>KDV</cbc:Name></cac:TaxScheme></cac:TaxCategory></cac:TaxSubtotal><cac:TaxSubtotal><cbc:TaxableAmount currencyID=\"TRY\">1500.00</cbc:TaxableAmount><cbc:TaxAmount currencyID=\"TRY\">200.00</cbc:TaxAmount><cbc:Percent>20.00</cbc:Percent><cac:TaxCategory><cac:TaxScheme><cbc:Name>ÖTV</cbc:Name></cac:TaxScheme></cac:TaxCategory></cac:TaxSubtotal>",
            );
        let read = read_ubl(xml.as_bytes()).unwrap();
        assert!(
            !read.issues.iter().any(|issue| issue.field == "taxAmount"),
            "the line VAT must match the KDV subtotal: {:?}",
            read.issues
        );
    }

    #[test]
    fn extracts_the_fields_of_a_ubl_invoice() {
        let read = read_ubl(invoice_20().as_bytes()).unwrap();
        let invoice = &read.invoice;
        assert_eq!(invoice.number, "GIB2026000000001");
        assert_eq!(invoice.ettn, "7f4e1c2a-0000-4000-8000-000000000001");
        assert_eq!(invoice.issue_date, "2026-03-14");
        assert_eq!(invoice.profile, "TICARIFATURA");
        assert_eq!(invoice.supplier.name, "Örnek Danışmanlık A.Ş.");
        assert_eq!(invoice.supplier.tax_id, "1234567890");
        assert_eq!(invoice.supplier.tax_id_scheme, "VKN");
        assert_eq!(invoice.supplier.city, "İstanbul");
        assert_eq!(invoice.customer.name, "Alıcı Ltd. Şti.");
        assert_eq!(invoice.lines.len(), 1);
        assert_eq!(invoice.lines[0].name, "Danışmanlık hizmeti");
        assert_eq!(invoice.lines[0].quantity, 10.0);
        assert_eq!(invoice.lines[0].unit_price, 150.0);
        assert_eq!(invoice.lines[0].line_total, 1500.0);
        assert_eq!(invoice.lines[0].vat_rate, 20.0);
        assert_eq!(invoice.tax_amount, 300.0);
        assert_eq!(invoice.payable_amount, 1800.0);
        assert!(read.issues.is_empty(), "issues: {:?}", read.issues);
    }

    #[test]
    fn flags_arithmetic_that_does_not_add_up() {
        // The document-level TaxAmount says 250 while the lines carry 300.
        let broken = invoice_20().replace(
            r#"<cbc:TaxAmount currencyID="TRY">300.00</cbc:TaxAmount>
    <cac:TaxSubtotal>"#,
            r#"<cbc:TaxAmount currencyID="TRY">250.00</cbc:TaxAmount>
    <cac:TaxSubtotal>"#,
        );
        let read = read_ubl(broken.as_bytes()).unwrap();
        assert!(read.issues.iter().any(|issue| issue.field == "taxAmount"), "issues: {:?}", read.issues);
    }

    #[test]
    fn reads_a_withholding_invoice_with_two_rates() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Invoice xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:ID>TEVK2026000000002</cbc:ID>
  <cbc:UUID>ettn-2</cbc:UUID>
  <cbc:IssueDate>2026-04-01</cbc:IssueDate>
  <cbc:DocumentCurrencyCode>TRY</cbc:DocumentCurrencyCode>
  <cac:AccountingSupplierParty><cac:Party><cac:PartyName><cbc:Name>Satıcı A.Ş.</cbc:Name></cac:PartyName></cac:Party></cac:AccountingSupplierParty>
  <cac:TaxTotal><cbc:TaxAmount>210.00</cbc:TaxAmount>
    <cac:TaxSubtotal><cbc:TaxableAmount>1500.00</cbc:TaxableAmount><cbc:TaxAmount>150.00</cbc:TaxAmount><cbc:Percent>10.00</cbc:Percent></cac:TaxSubtotal>
    <cac:TaxSubtotal><cbc:TaxableAmount>600.00</cbc:TaxableAmount><cbc:TaxAmount>60.00</cbc:TaxAmount><cbc:Percent>10.00</cbc:Percent></cac:TaxSubtotal>
  </cac:TaxTotal>
  <cac:WithholdingTaxTotal><cbc:TaxAmount>105.00</cbc:TaxAmount>
    <cac:TaxSubtotal><cbc:TaxableAmount>210.00</cbc:TaxableAmount><cbc:TaxAmount>105.00</cbc:TaxAmount><cbc:Percent>50.00</cbc:Percent>
      <cac:TaxCategory><cac:TaxScheme><cbc:Name>KDV Tevkifatı</cbc:Name></cac:TaxScheme></cac:TaxCategory></cac:TaxSubtotal>
  </cac:WithholdingTaxTotal>
  <cac:LegalMonetaryTotal>
    <cbc:LineExtensionAmount>2100.00</cbc:LineExtensionAmount>
    <cbc:TaxExclusiveAmount>2100.00</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount>2310.00</cbc:TaxInclusiveAmount>
    <cbc:PayableAmount>2205.00</cbc:PayableAmount>
  </cac:LegalMonetaryTotal>
  <cac:InvoiceLine>
    <cbc:ID>1</cbc:ID>
    <cbc:InvoicedQuantity>100</cbc:InvoicedQuantity>
    <cbc:LineExtensionAmount>1500.00</cbc:LineExtensionAmount>
    <cac:TaxTotal><cbc:TaxAmount>150.00</cbc:TaxAmount><cac:TaxSubtotal><cbc:TaxableAmount>1500.00</cbc:TaxableAmount><cbc:TaxAmount>150.00</cbc:TaxAmount><cbc:Percent>10.00</cbc:Percent></cac:TaxSubtotal></cac:TaxTotal>
    <cac:Item><cbc:Name>Hizmet A</cbc:Name></cac:Item>
    <cac:Price><cbc:PriceAmount>15.00</cbc:PriceAmount></cac:Price>
  </cac:InvoiceLine>
  <cac:InvoiceLine>
    <cbc:ID>2</cbc:ID>
    <cbc:InvoicedQuantity>60</cbc:InvoicedQuantity>
    <cbc:LineExtensionAmount>600.00</cbc:LineExtensionAmount>
    <cac:TaxTotal><cbc:TaxAmount>60.00</cbc:TaxAmount><cac:TaxSubtotal><cbc:TaxableAmount>600.00</cbc:TaxableAmount><cbc:TaxAmount>60.00</cbc:TaxAmount><cbc:Percent>10.00</cbc:Percent></cac:TaxSubtotal></cac:TaxTotal>
    <cac:Item><cbc:Name>Hizmet B</cbc:Name></cac:Item>
    <cac:Price><cbc:PriceAmount>10.00</cbc:PriceAmount></cac:Price>
  </cac:InvoiceLine>
</Invoice>"#;
        let read = read_ubl(xml.as_bytes()).unwrap();
        let invoice = &read.invoice;
        assert_eq!(invoice.tax_subtotals.len(), 2);
        assert_eq!(invoice.withholding_amount, 105.0);
        assert_eq!(invoice.withholding_subtotals[0].name, "KDV Tevkifatı");
        assert_eq!(invoice.withholding_subtotals[0].rate, 50.0);
        assert_eq!(invoice.payable_amount, 2205.0);
        assert!(read.issues.is_empty(), "issues: {:?}", read.issues);
    }

    #[test]
    fn reports_an_unknown_vat_rate() {
        let xml = invoice_20()
            .replace(
                "<cbc:Percent>20.00</cbc:Percent>\n      <cac:TaxCategory>",
                "<cbc:Percent>13.00</cbc:Percent>\n      <cac:TaxCategory>",
            )
            .replace(
                "<cbc:TaxAmount>300.00</cbc:TaxAmount><cbc:Percent>20.00</cbc:Percent>",
                "<cbc:TaxAmount>195.00</cbc:TaxAmount><cbc:Percent>13.00</cbc:Percent>",
            );
        let read = read_ubl(xml.as_bytes()).unwrap();
        assert!(
            read.issues.iter().any(|issue| issue.field == "line:1.rate"),
            "an unknown rate must be reported: {:?}",
            read.issues
        );
    }

    #[test]
    fn reads_a_zip_with_two_invoices() {
        use crate::zip::ZipWriter;
        let mut zip = ZipWriter::new();
        zip.add_text("invoice1.xml", &invoice_20());
        zip.add_text("invoice2.xml", &invoice_20().replace("GIB2026000000001", "GIB2026000000002"));
        zip.add_text("readme.txt", "not an invoice");
        let reads = read_ubl_package(&zip.finish()).unwrap();
        assert_eq!(reads.len(), 2);
        assert_eq!(reads[0].invoice.number, "GIB2026000000001");
        assert_eq!(reads[1].invoice.number, "GIB2026000000002");
    }

    #[test]
    fn renders_a_deterministic_document_with_the_totals() {
        let read = read_ubl(invoice_20().as_bytes()).unwrap();
        let document = invoice_to_document(&read.invoice);
        let text = document.plain_text();
        assert!(text.contains("GIB2026000000001"));
        assert!(text.contains("Örnek Danışmanlık A.Ş."));
        assert!(text.contains("Danışmanlık hizmeti"));
        assert!(text.contains("1.500,00"), "totals use Turkish formatting: {text}");
        assert!(text.contains("300,00"));
        assert!(text.contains("1.800,00"));
        // The totals block is bold.
        assert!(document
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Paragraph { runs, .. } if runs.iter().any(|run| run.bold))));
        // The rendered document exports to PDF through the existing pipeline.
        let pdf = crate::layout::document_to_pdf(&document);
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn builds_a_vat_summary_workbook() {
        let first = read_ubl(invoice_20().as_bytes()).unwrap().invoice;
        let mut second = read_ubl(invoice_20().as_bytes()).unwrap().invoice;
        second.number = "GIB2026000000003".into();
        second.issue_date = "2026-04-02".into();
        second.supplier.name = "Başka A.Ş.".into();
        let workbook = vat_summary_workbook(&[first, second]);
        let sheet = &workbook.sheets[0];
        assert_eq!(sheet.name, "KDV Özeti");
        assert_eq!(sheet.cells["A1"].value, CellValue::Text("Ay".into()));
        // Header, two month/supplier rows and a totals row.
        let rows: Vec<&String> = sheet.cells.keys().filter(|address| address.starts_with('A')).collect();
        assert_eq!(rows.len(), 4, "rows: {rows:?}");
        // The totals row sums both invoices' base and VAT.
        assert_eq!(sheet.cells["E4"].value, CellValue::Number(3000.0));
        assert_eq!(sheet.cells["F4"].value, CellValue::Number(600.0));
        assert_eq!(sheet.cells["H4"].value, CellValue::Number(3600.0));
        // It exports to XLSX.
        let package = crate::xlsx::write_xlsx_package(&workbook).unwrap();
        assert!(package.bytes.starts_with(b"PK"));
    }

    #[test]
    fn rejects_xml_that_is_not_an_invoice() {
        let error = read_ubl(b"<?xml version=\"1.0\"?><notes><note>hi</note></notes>").unwrap_err();
        assert!(error.to_string().contains("not a UBL invoice"), "{error}");
    }
}
