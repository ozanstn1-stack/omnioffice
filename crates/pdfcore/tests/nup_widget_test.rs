//! Form-field widgets reach the paper only when their appearance can be trusted.

#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/markup_fixtures.rs"]
mod markup_fixtures;

use fixtures::{impose, page_content, sheets, Fixture, PageSpec, TestDir};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use markup_fixtures::{appearance, blocks, form, markup, state_dictionary, xobject};
use pdfcore::nup::NupOptions;

fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

/// A widget dictionary with an `/AP /N` stream and the given field type.
fn widget(kind: Option<&str>, rect: [f64; 4], flags: Option<i64>, ap: ObjectId) -> Dictionary {
    let mut annot = markup("Widget", rect, flags, ap);
    if let Some(kind) = kind {
        annot.set("FT", Object::Name(kind.as_bytes().to_vec()));
    }
    annot
}

/// Marks the catalog `NeedAppearances`, as a producer that has not regenerated
/// appearances would.
fn need_appearances(fixture: &mut Fixture) {
    let catalog = fixture.catalog_id();
    fixture.patch(catalog, "AcroForm", Object::Dictionary(dictionary! { "NeedAppearances" => Object::Boolean(true) }));
}

/// A checkbox with `On`/`Off` state appearances; returns the widget dictionary.
fn checkbox(fixture: &mut Fixture, selected: Option<&str>) -> Dictionary {
    let on = fixture.add_object(form(b"5 5 90 40 re S", [0.0, 0.0, 100.0, 50.0], None));
    let off = fixture.add_object(form(b"", [0.0, 0.0, 100.0, 50.0], None));
    let states = state_dictionary(fixture, &[("On", on), ("Off", off)]);
    let mut annot = widget(Some("Btn"), [100.0, 100.0, 200.0, 150.0], Some(4), on);
    annot.set("AP", dictionary! { "N" => Object::Reference(states) });
    if let Some(selected) = selected {
        annot.set("AS", Object::Name(selected.as_bytes().to_vec()));
    }
    annot
}

fn content_of(doc: &Document, sheet: ObjectId, name: &str) -> Vec<u8> {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    doc.get_object(id).and_then(|object| object.as_stream()).expect("stream").content.clone()
}

#[test]
fn a_checkbox_draws_its_on_state() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let annot = checkbox(&mut fixture, Some("On"));
    fixture.annot(0, annot);
    let input = dir.path("on.pdf");
    fixture.write(&input);

    let out = dir.path("on-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1, "a printed checkbox is flattened");
    assert_eq!(content_of(&doc, sheet, &drawn[0].name).as_slice(), b"5 5 90 40 re S");
}

#[test]
fn a_checkbox_draws_its_off_state() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let annot = checkbox(&mut fixture, Some("Off"));
    fixture.annot(0, annot);
    let input = dir.path("off.pdf");
    fixture.write(&input);

    let out = dir.path("off-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    assert!(content_of(&doc, sheet, &drawn[0].name).is_empty(), "the Off appearance is the empty box");
}

#[test]
fn a_widget_without_a_selected_state_is_skipped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let annot = checkbox(&mut fixture, None);
    fixture.annot(0, annot);
    let input = dir.path("nostate.pdf");
    fixture.write(&input);

    let out = dir.path("nostate-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert!(painted(&doc, sheet).is_empty(), "no /AS means no appearance to pick");
}

#[test]
fn a_text_field_draws_when_its_appearance_is_current() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, widget(Some("Tx"), [100.0, 100.0, 300.0, 150.0], Some(4), ap));
    let input = dir.path("text.pdf");
    fixture.write(&input);

    let out = dir.path("text-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1);
}

#[test]
fn need_appearances_holds_back_text_but_not_buttons() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, widget(Some("Tx"), [100.0, 100.0, 300.0, 150.0], Some(4), ap));
    let checkbox = checkbox(&mut fixture, Some("On"));
    fixture.annot(0, checkbox);
    need_appearances(&mut fixture);
    let input = dir.path("stale.pdf");
    fixture.write(&input);

    let out = dir.path("stale-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let drawn = painted(&doc, sheets(&doc)[0].1);
    assert_eq!(drawn.len(), 1, "only the button keeps a trustworthy appearance");
    assert_eq!(content_of(&doc, sheets(&doc)[0].1, &drawn[0].name).as_slice(), b"5 5 90 40 re S");
}

#[test]
fn an_unsigned_signature_field_stays_a_placeholder() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, widget(Some("Sig"), [100.0, 100.0, 300.0, 150.0], Some(4), ap));
    let input = dir.path("unsigned.pdf");
    fixture.write(&input);

    let out = dir.path("unsigned-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert!(painted(&doc, sheets(&doc)[0].1).is_empty(), "an unsigned signature is not printed");
}

#[test]
fn a_signed_signature_field_is_drawn() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = widget(Some("Sig"), [100.0, 100.0, 300.0, 150.0], Some(4), ap);
    annot.set("V", Object::Dictionary(dictionary! { "Type" => "Sig", "Name" => Object::string_literal("Ozan") }));
    fixture.annot(0, annot);
    let input = dir.path("signed.pdf");
    fixture.write(&input);

    let out = dir.path("signed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "a signed field shows its appearance");
}

#[test]
fn the_field_type_is_inherited_from_the_parent() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    // The widget carries no /FT of its own; its parent field does.
    let field =
        fixture.add_object(Object::Dictionary(dictionary! { "FT" => "Tx", "T" => Object::string_literal("name") }));
    let mut annot = widget(None, [100.0, 100.0, 300.0, 150.0], Some(4), ap);
    annot.set("Parent", Object::Reference(field));
    fixture.annot(0, annot);
    let input = dir.path("inherit.pdf");
    fixture.write(&input);

    let out = dir.path("inherit-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "a current appearance is printed");

    // The same inherited type is held back once appearances may be stale.
    let stale_input = dir.path("inherit-stale.pdf");
    need_appearances(&mut fixture);
    fixture.write(&stale_input);
    let stale_out = dir.path("inherit-stale-out.pdf");
    impose(&stale_input, &stale_out, &NupOptions::default());
    let doc = Document::load(&stale_out).expect("load output");
    assert!(painted(&doc, sheets(&doc)[0].1).is_empty(), "the inherited /FT was found");
}

#[test]
fn a_parent_cycle_terminates() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = widget(None, [100.0, 100.0, 300.0, 150.0], Some(4), ap);
    let widget_id = fixture.add_object(Object::Dictionary(annot.clone()));
    let field = fixture.add_object(Object::Dictionary(dictionary! {
        "Parent" => Object::Reference(widget_id),
        "T" => Object::string_literal("loop"),
    }));
    annot.set("Parent", Object::Reference(field));
    fixture.annot(0, annot);
    let input = dir.path("cycle.pdf");
    fixture.write(&input);

    let out = dir.path("cycle-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("a parent cycle must not hang the imposition");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "no /FT found, the appearance is printed");
}

#[test]
fn a_widget_without_the_print_flag_is_skipped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, widget(Some("Btn"), [100.0, 100.0, 300.0, 150.0], None, ap));
    let input = dir.path("noprint.pdf");
    fixture.write(&input);

    let out = dir.path("noprint-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert!(painted(&doc, sheets(&doc)[0].1).is_empty(), "Print is required, as the spec says");
}

#[test]
fn a_push_button_draws_its_single_appearance() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = widget(Some("Btn"), [100.0, 100.0, 300.0, 150.0], Some(4), ap);
    annot.set("Ff", Object::Integer(65536)); // pushbutton
    fixture.annot(0, annot);
    let input = dir.path("push.pdf");
    fixture.write(&input);

    let out = dir.path("push-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1, "a push button has one appearance");
}

#[test]
fn widgets_and_markup_keep_the_annotation_order() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, markup("Square", [50.0, 400.0, 150.0, 450.0], Some(4), ap));
    fixture.annot(0, widget(Some("Btn"), [100.0, 100.0, 300.0, 150.0], Some(4), ap));
    fixture.annot(0, markup("Ink", [50.0, 500.0, 150.0, 550.0], Some(4), ap));
    let input = dir.path("mixed.pdf");
    fixture.write(&input);

    let out = dir.path("mixed-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 3, "markup and the widget are painted together");
    assert_eq!(drawn[0].name, "AnX0");
    assert_eq!(drawn[1].name, "AnX1");
    assert_eq!(drawn[2].name, "AnX2");
    let widths: Vec<f64> = drawn.iter().map(|block| block.clip[2] - block.clip[0]).collect();
    assert!(widths.iter().all(|width| *width > 1.0), "every block has a real clip: {widths:?}");
}

#[test]
fn a_document_without_fields_is_unchanged() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    fixture.annot(0, markup("Square", [100.0, 100.0, 200.0, 200.0], Some(0), ap));
    let input = dir.path("plain.pdf");
    fixture.write(&input);

    let out = dir.path("plain-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    assert!(
        matches!(doc.get_dictionary(sheet).unwrap().get(b"Contents"), Ok(Object::Reference(_))),
        "nothing is painted, so no second stream is added"
    );
}

#[test]
fn a_dangling_parent_does_not_stop_the_widget() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut annot = widget(None, [100.0, 100.0, 300.0, 150.0], Some(4), ap);
    annot.set("Parent", Object::Reference((9999, 0)));
    fixture.annot(0, annot);
    let input = dir.path("dangling.pdf");
    fixture.write(&input);

    let out = dir.path("dangling-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("a dangling parent is tolerated");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 1);
}

#[test]
fn widget_opacity_and_optional_content_apply() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let ocg = fixture
        .add_object(Object::Dictionary(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("layer") }));
    let mut annot = widget(Some("Btn"), [100.0, 100.0, 300.0, 150.0], Some(4), ap);
    annot.set("CA", Object::Real(0.5));
    annot.set("OC", Object::Reference(ocg));
    fixture.annot(0, annot);
    let input = dir.path("styled.pdf");
    fixture.write(&input);

    let out = dir.path("styled-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    let drawn = painted(&doc, sheet);
    assert_eq!(drawn.len(), 1);
    assert!(drawn[0].graphics_state.is_some(), "the opacity becomes a graphics state");
    assert!(drawn[0].marked_content.is_some(), "the optional content wraps the block");
}

#[test]
fn a_broken_field_type_does_not_stop_the_other_widgets() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&page_specs(2));
    let ap = fixture.add_object(appearance([0.0, 0.0, 100.0, 50.0], None));
    let mut broken = widget(None, [100.0, 100.0, 200.0, 150.0], Some(4), ap);
    broken.set("FT", Object::Integer(5));
    fixture.annot(0, broken);
    fixture.annot(0, widget(Some("Btn"), [300.0, 300.0, 400.0, 350.0], Some(4), ap));
    let input = dir.path("broken.pdf");
    fixture.write(&input);

    let out = dir.path("broken-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert_eq!(painted(&doc, sheets(&doc)[0].1).len(), 2, "an unreadable /FT falls back to the appearance");
}
