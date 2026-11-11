//! ODP master pages (4.3a).
//!
//! A deck's slide masters become `style:master-page` elements in styles.xml,
//! each with its name, its background colour and its shapes, and every slide
//! names its own master in `draw:master-page-name`. A deck without masters is
//! written exactly as before: one "Default" master page. On import, the master
//! pages come back as [`SlideMaster`]s unless the file has a single master page
//! without shapes, which is what most presentations (and older exports) hold.
use super::slides_read::read_odp_shapes;
use super::slides_write::*;
use super::slides_write_file::{odp_footer_shapes, OdpImport};
use super::slides_write_objects::*;
use super::*;
use crate::xml::escape_attr;

/// The `style:name` of a master page: the master's id when it is a valid NCName.
fn page_name(master: &SlideMaster, index: usize) -> String {
    if is_ncname(&master.id) {
        master.id.clone()
    } else {
        format!("Master{}", index + 1)
    }
}

/// The master page a slide names: its own master, else the deck's first one.
pub(super) fn master_page_name(deck: &Deck, slide: &Slide) -> String {
    let index =
        slide.master_id.as_deref().and_then(|id| deck.masters.iter().position(|master| master.id == id)).unwrap_or(0);
    deck.masters.get(index).map(|master| page_name(master, index)).unwrap_or_else(|| "Default".into())
}

/// A `#RRGGBB` background, the only kind a master page fill takes here.
fn solid_colour(background: Option<&str>) -> Option<&str> {
    let value = background?.trim();
    (value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())).then_some(value)
}

/// The page fill of the deck's theme, for a master without a background of its own.
fn theme_fill(theme: &str) -> &'static str {
    match theme {
        "dark" => "#0F172A",
        "business" => "#F8FAFC",
        "education" => "#FEFCE8",
        _ => "#FFFFFF",
    }
}

/// The footer flags of a master page's drawing properties and the footer frames it carries.
fn footer_parts(deck: &Deck) -> (String, String) {
    let Some(footer) = deck.footer.as_ref().filter(|footer| footer.enabled) else { return Default::default() };
    let mut flags = String::new();
    if footer.show_text {
        flags.push_str(" presentation:display-footer=\"true\"");
    }
    if footer.show_slide_number {
        flags.push_str(" presentation:display-page-number=\"true\"");
    }
    if footer.show_date {
        flags.push_str(" presentation:display-date-time=\"true\"");
    }
    (flags, odp_footer_shapes(deck, footer))
}

/// The `office:master-styles` children: the "Default" page for a deck without
/// masters, else one page per master. Every page carries the deck footer.
pub(super) fn master_pages_xml(
    deck: &Deck,
    ids: &mut OdpShapeIds,
    auto: &mut AutoStyles,
    pictures: &mut Vec<(String, Vec<u8>)>,
) -> String {
    let (flags, shapes) = footer_parts(deck);
    let page_fill = theme_fill(&deck.theme);
    let page = |name: &str, display: Option<&str>, fill: &str, objects: &str| {
        let display = display.map(|text| format!(" style:display-name=\"{}\"", escape_attr(text))).unwrap_or_default();
        format!(
            "<style:master-page style:name=\"{name}\"{display} style:page-layout-name=\"pl1\"><style:drawing-page-properties draw:fill=\"solid\" draw:fill-color=\"{fill}\"{flags}/>{shapes}{objects}</style:master-page>"
        )
    };
    if deck.masters.is_empty() {
        return page("Default", None, page_fill, "");
    }
    let mut out = String::new();
    for (index, master) in deck.masters.iter().enumerate() {
        ids.reset_slide();
        plan_odp_ids(&master.objects, ids);
        collect_odp_pictures(&master.objects, pictures);
        let mut objects: Vec<&SlideObject> = master.objects.iter().collect();
        objects.sort_by_key(|object| object.z);
        let charts = std::collections::HashMap::new();
        let shapes_xml: String =
            objects.into_iter().map(|object| slide_object_xml(object, ids, auto, &charts)).collect();
        let fill = solid_colour(master.background.as_deref()).unwrap_or(page_fill);
        out.push_str(&page(&page_name(master, index), Some(&master.name), fill, &shapes_xml));
    }
    out
}

/// The footer, date and slide-number frames a master page carries for the deck footer.
fn is_footer_placeholder(node: &XmlNode) -> bool {
    matches!(node.attr_any_ns("class"), Some("footer" | "date-time" | "page-number"))
}

/// The master pages of styles.xml as slide masters, or none when the file has a
/// single master page without shapes of its own.
pub(super) fn read_odp_masters(
    styles_root: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Vec<SlideMaster> {
    let mut pages = Vec::new();
    styles_root.find_all("master-page", &mut pages);
    let masters: Vec<SlideMaster> = pages
        .into_iter()
        .filter_map(|page| {
            let id = page.attr_any_ns("name")?.to_string();
            let mut shapes = page.clone();
            shapes.children.retain(|child| !is_footer_placeholder(child));
            let background = page
                .children
                .iter()
                .find(|child| child.local_name() == "drawing-page-properties")
                .and_then(|properties| solid_colour(properties.attr_any_ns("fill-color")).map(str::to_string));
            Some(SlideMaster {
                name: page.attr_any_ns("display-name").unwrap_or(&id).to_string(),
                objects: read_odp_shapes(&shapes, zip, import, warnings),
                background,
                id,
                ..Default::default()
            })
        })
        .collect();
    if masters.len() == 1 && masters[0].objects.is_empty() {
        return Vec::new();
    }
    masters
}

/// The id of the imported master a page names, when there is one.
pub(super) fn slide_master_id(deck: &Deck, page: &XmlNode) -> Option<String> {
    let name = page.attr_any_ns("master-page-name")?;
    deck.master(name).map(|master| master.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::odf::{read_odp, write_odp};

    fn styles_of(bytes: &[u8]) -> String {
        ZipReader::open(bytes.to_vec()).unwrap().read_text("styles.xml").unwrap()
    }

    fn two_master_deck() -> Deck {
        let mut deck = Deck::new_blank("Masters");
        let logo = SlideObject::new("rect", 20.0, 20.0, 80.0, 40.0);
        deck.masters = vec![
            SlideMaster {
                id: "Title".into(),
                name: "Title master".into(),
                background: Some("#0F172A".into()),
                objects: vec![logo],
                ..Default::default()
            },
            SlideMaster {
                id: "Body".into(),
                name: "Body master".into(),
                background: Some("#FEFCE8".into()),
                ..Default::default()
            },
        ];
        deck.slides = vec![
            Slide { master_id: Some("Title".into()), ..Default::default() },
            Slide { master_id: Some("Body".into()), ..Default::default() },
            Slide::default(),
        ];
        deck
    }

    #[test]
    fn every_master_is_written_and_each_slide_names_its_own() {
        let bytes = write_odp(&two_master_deck()).unwrap();
        let styles = styles_of(&bytes);
        assert!(styles.contains("style:name=\"Title\" style:display-name=\"Title master\""), "{styles}");
        assert!(styles.contains("draw:fill-color=\"#0F172A\""));
        assert!(styles.contains("style:name=\"Body\" style:display-name=\"Body master\""));
        assert!(styles.contains("draw:fill-color=\"#FEFCE8\""));
        assert!(!styles.contains("style:name=\"Default\""));
        let content = ZipReader::open(bytes).unwrap().read_text("content.xml").unwrap();
        let names: Vec<&str> =
            content.match_indices("draw:master-page-name=\"").map(|(at, _)| &content[at + 23..at + 27]).collect();
        // A slide without a master of its own takes the deck's first master.
        assert_eq!(names, ["Titl", "Body", "Titl"]);
    }

    #[test]
    fn masters_round_trip_with_their_names_backgrounds_and_shapes() {
        let read = read_odp(&write_odp(&two_master_deck()).unwrap()).unwrap().deck;
        let masters: Vec<_> = read
            .masters
            .iter()
            .map(|m| (m.id.as_str(), m.name.as_str(), m.background.as_deref(), m.objects.len()))
            .collect();
        assert_eq!(
            masters,
            [("Title", "Title master", Some("#0F172A"), 1), ("Body", "Body master", Some("#FEFCE8"), 0)]
        );
        let slide_masters: Vec<_> = read.slides.iter().map(|slide| slide.master_id.as_deref()).collect();
        assert_eq!(slide_masters, [Some("Title"), Some("Body"), Some("Title")]);
    }

    #[test]
    fn a_deck_without_masters_keeps_the_single_default_page_and_reads_back_without_masters() {
        let mut deck = Deck::new_blank("Plain");
        deck.footer = Some(SlideFooter { enabled: true, show_slide_number: true, ..Default::default() });
        let bytes = write_odp(&deck).unwrap();
        let styles = styles_of(&bytes);
        assert_eq!(styles.matches("<style:master-page ").count(), 1);
        assert!(styles.contains("<style:master-page style:name=\"Default\" style:page-layout-name=\"pl1\"><style:drawing-page-properties draw:fill=\"solid\" draw:fill-color=\"#FFFFFF\" presentation:display-page-number=\"true\"/>"), "{styles}");
        let read = read_odp(&bytes).unwrap().deck;
        assert!(read.masters.is_empty());
        assert!(read.slides.iter().all(|slide| slide.master_id.is_none()));
        // The slide-number frame on the master is the footer, not a master shape.
        assert!(read.footer.is_some_and(|footer| footer.show_slide_number));
    }
}
