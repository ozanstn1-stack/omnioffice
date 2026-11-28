//! Default constructors and built-in style catalogue for the document model.

use crate::model::ParaStyle;

pub(crate) fn default_chart_width_pt() -> f64 {
    420.0
}

pub(crate) fn default_chart_height_pt() -> f64 {
    260.0
}

pub(crate) fn default_center() -> String {
    "center".into()
}

/// The style catalogue every Writer document starts with (fully original design).
pub fn default_styles() -> Vec<ParaStyle> {
    let mut list = Vec::new();
    let normal = ParaStyle {
        font: Some("Calibri".into()),
        size_pt: Some(11.0),
        line_spacing: Some(1.15),
        space_after_pt: Some(8.0),
        color: Some("#1f2328".into()),
        ..Default::default()
    };
    list.push(normal);

    let mut title = ParaStyle {
        id: "Title".into(),
        name: "Title".into(),
        based_on: Some("Normal".into()),
        next: Some("Subtitle".into()),
        ..Default::default()
    };
    title.font = Some("Calibri Light".into());
    title.size_pt = Some(28.0);
    title.bold = Some(true);
    title.color = Some("#0f172a".into());
    title.space_after_pt = Some(6.0);
    list.push(title);

    let mut subtitle = ParaStyle {
        id: "Subtitle".into(),
        name: "Subtitle".into(),
        based_on: Some("Normal".into()),
        next: Some("Normal".into()),
        ..Default::default()
    };
    subtitle.size_pt = Some(15.0);
    subtitle.italic = Some(true);
    subtitle.color = Some("#475569".into());
    subtitle.space_after_pt = Some(14.0);
    list.push(subtitle);

    for (index, size) in [(1u32, 20.0f64), (2, 16.0), (3, 13.0), (4, 11.5), (5, 11.0), (6, 10.5)] {
        let mut heading = ParaStyle {
            id: format!("Heading{index}"),
            name: format!("Heading {index}"),
            based_on: Some("Normal".into()),
            next: Some("Normal".into()),
            ..Default::default()
        };
        heading.font = Some("Calibri Light".into());
        heading.size_pt = Some(size);
        heading.bold = Some(true);
        heading.color = Some(if index == 1 { "#1d4ed8".into() } else { "#334155".to_string() });
        heading.space_before_pt = Some(if index == 1 { 16.0 } else { 12.0 });
        heading.space_after_pt = Some(4.0);
        heading.keep_with_next = Some(true);
        heading.outline_level = Some(index - 1);
        list.push(heading);
    }

    let mut quote =
        ParaStyle { id: "Quote".into(), name: "Quote".into(), based_on: Some("Normal".into()), ..Default::default() };
    quote.italic = Some(true);
    quote.color = Some("#334155".into());
    quote.indent_left_pt = Some(24.0);
    quote.indent_right_pt = Some(24.0);
    quote.space_before_pt = Some(8.0);
    quote.space_after_pt = Some(8.0);
    list.push(quote);

    let mut caption = ParaStyle {
        id: "Caption".into(),
        name: "Caption".into(),
        based_on: Some("Normal".into()),
        ..Default::default()
    };
    caption.size_pt = Some(9.5);
    caption.italic = Some(true);
    caption.align = Some("center".into());
    caption.color = Some("#64748b".into());
    list.push(caption);

    let mut code =
        ParaStyle { id: "Code".into(), name: "Code".into(), based_on: Some("Normal".into()), ..Default::default() };
    code.font = Some("Consolas".into());
    code.size_pt = Some(10.0);
    code.space_after_pt = Some(0.0);
    list.push(code);

    list
}
