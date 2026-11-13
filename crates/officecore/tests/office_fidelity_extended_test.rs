//! Extended round-trip fidelity corpus for DOCX, ODT, XLSX, PPTX, and ODP.
//!
//! Complements `office_fidelity.rs` by measuring images, charts with cached
//! data, threaded comments and replies, headers/footers, list numbering,
//! section breaks, freeze panes, hyperlinks, structured tables, print settings,
//! slide masters, nested groups, image cropping, and animations.
//!
//! Run with: `cargo test -p officecore --test office_fidelity_extended_test -- --nocapture`

mod fidelity_extended;

use fidelity_extended::{
    collect_calc_stats, collect_deck_stats, collect_writer_stats, extended_deck, extended_workbook,
    extended_writer_document,
};
use officecore::docx::{read_docx, write_docx};
use officecore::odf::{read_odp, read_odt, write_odp, write_odt};
use officecore::pptx::{read_pptx, write_pptx};
use officecore::xlsx::{read_workbook_bytes, write_xlsx};

#[test]
fn docx_extended_fidelity_preserves_images_comments_headers_numbering_and_sections() {
    let doc = extended_writer_document(true);
    let before = collect_writer_stats(&doc);
    let read = read_docx(&write_docx(&doc).expect("write DOCX")).expect("read DOCX");
    let after = collect_writer_stats(&read.document);

    println!(
        "DOCX-EXT images {}/{}  bytes {}/{}  comments {}/{}  replies {}/{}  anchors {}/{}  headers {}/{}  footers {}/{}  bullets {}/{}  numbered {}/{}  sections {}/{}",
        after.images, before.images, after.image_bytes, before.image_bytes, after.comments, before.comments,
        after.comment_replies, before.comment_replies, after.anchored_runs, before.anchored_runs,
        after.headers, before.headers, after.footers, before.footers,
        after.bullet_items, before.bullet_items, after.numbered_items, before.numbered_items, after.sections, before.sections,
    );
    assert!(read.warnings.is_empty(), "warnings: {:?}", read.warnings);
    assert_eq!(after, before);
}

#[test]
fn odt_extended_fidelity_preserves_images_comments_headers_and_lists() {
    let doc = extended_writer_document(false);
    let before = collect_writer_stats(&doc);
    let read = read_odt(&write_odt(&doc).expect("write ODT")).expect("read ODT");
    let after = collect_writer_stats(&read.document);

    println!(
        "ODT-EXT  images {}/{}  bytes {}/{}  comments {}/{}  replies {}/{}  resolved {}/{}  anchors {}/{}  headers {}/{}  footers {}/{}  lists {}/{}",
        after.images, before.images, after.image_bytes, before.image_bytes, after.comments, before.comments,
        after.comment_replies, before.comment_replies, after.resolved_comments, before.resolved_comments,
        after.anchored_runs, before.anchored_runs, after.headers, before.headers, after.footers, before.footers,
        after.list_items, before.list_items,
    );
    assert!(read.warnings.is_empty(), "warnings: {:?}", read.warnings);
    assert_eq!(
        (
            after.images,
            after.image_bytes,
            after.comments,
            after.comment_replies,
            after.resolved_comments,
            after.anchored_runs,
            after.headers,
            after.footers,
            after.list_items,
        ),
        (
            before.images,
            before.image_bytes,
            before.comments,
            before.comment_replies,
            before.resolved_comments,
            before.anchored_runs,
            before.headers,
            before.footers,
            before.list_items,
        )
    );
}

#[test]
fn xlsx_extended_fidelity_preserves_images_charts_tables_links_print_and_panes() {
    let wb = extended_workbook();
    let before = collect_calc_stats(&wb);
    let read = read_workbook_bytes(&write_xlsx(&wb).expect("write XLSX")).expect("read XLSX");
    let after = collect_calc_stats(&read.workbook);

    println!(
        "XLSX-EXT images {}/{}  charts {}/{}  cache_pts {}/{}  tables {}/{}  links {}/{}  tooltips {}/{}  comments {}/{}  freeze {}x{}/{}x{}  breaks {}/{}  names {}/{}  print {}/{}  prot {}/{}",
        after.images, before.images, after.charts, before.charts, after.chart_cache_points, before.chart_cache_points,
        after.tables, before.tables, after.hyperlinks, before.hyperlinks, after.tooltips, before.tooltips,
        after.comments, before.comments, after.freeze_rows, after.freeze_cols, before.freeze_rows, before.freeze_cols,
        after.breaks, before.breaks, after.names, before.names,
        after.print_area + after.print_titles, before.print_area + before.print_titles, after.protected, before.protected,
    );
    assert_eq!(after, before);
}

#[test]
fn pptx_extended_fidelity_preserves_masters_groups_crops_charts_and_animations() {
    let deck = extended_deck();
    let before = collect_deck_stats(&deck);
    let read = read_pptx(&write_pptx(&deck).expect("write PPTX")).expect("read PPTX");
    let after = collect_deck_stats(&read.deck);

    println!(
        "PPTX-EXT masters {}/{}  layouts {}/{}  groups {}/{}  crops {}/{}  charts {}/{}  cache_pts {}/{}  anims {}/{}  notes {}/{}  transitions {}/{}  hidden {}/{}",
        after.masters, before.masters, after.layouts, before.layouts, after.groups, before.groups,
        after.cropped_images, before.cropped_images, after.charts, before.charts,
        after.chart_cache_points, before.chart_cache_points, after.animations, before.animations,
        after.notes, before.notes, after.transitions, before.transitions, after.hidden_slides, before.hidden_slides,
    );
    assert_eq!(after, before);
}

#[test]
fn odp_extended_fidelity_preserves_masters_groups_crops_charts_and_animations() {
    let deck = extended_deck();
    let before = collect_deck_stats(&deck);
    let read = read_odp(&write_odp(&deck).expect("write ODP")).expect("read ODP");
    let after = collect_deck_stats(&read.deck);

    println!(
        "ODP-EXT  masters {}/{}  groups {}/{}  crops {}/{}  charts {}/{}  cache_pts {}/{}  anims {}/{}  hidden {}/{}",
        after.masters,
        before.masters,
        after.groups,
        before.groups,
        after.cropped_images,
        before.cropped_images,
        after.charts,
        before.charts,
        after.chart_cache_points,
        before.chart_cache_points,
        after.animations,
        before.animations,
        after.hidden_slides,
        before.hidden_slides,
    );
    assert_eq!(
        (
            after.masters,
            after.groups,
            after.cropped_images,
            after.charts,
            after.chart_cache_points,
            after.animations,
            after.hidden_slides,
        ),
        (
            before.masters,
            before.groups,
            before.cropped_images,
            before.charts,
            before.chart_cache_points,
            before.animations,
            before.hidden_slides,
        )
    );
}
