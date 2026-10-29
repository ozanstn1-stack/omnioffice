//! The wire shape of an object edit (PDF Studio "Objects" panel): the front
//! end sends the action's tag and fields beside the object's address.

use pdfcore::forms::{ObjectAction, ObjectEdit};

fn parse(json: serde_json::Value) -> ObjectEdit {
    serde_json::from_value(json).expect("the flat {page, index, action, ...} shape")
}

#[test]
fn an_object_edit_reads_the_flat_shape_the_front_end_sends() {
    let edit = parse(serde_json::json!({ "page": 2, "index": 5, "action": "move", "dx": 10.0, "dy": -4.0 }));
    assert_eq!((edit.page, edit.index), (2, 5));
    assert!(matches!(edit.action, ObjectAction::Move { dx, dy } if dx == 10.0 && dy == -4.0));

    let edit = parse(serde_json::json!({ "page": 1, "index": 0, "action": "resize", "rect": [1.0, 2.0, 3.0, 4.0] }));
    assert!(matches!(edit.action, ObjectAction::Resize { rect } if rect == [1.0, 2.0, 3.0, 4.0]));

    let edit = parse(serde_json::json!({ "page": 1, "index": 3, "action": "delete" }));
    assert!(matches!(edit.action, ObjectAction::Delete));

    let edit = parse(serde_json::json!({ "page": 4, "index": 1, "action": "rotate", "degrees": 90.0 }));
    assert!(matches!(edit.action, ObjectAction::Rotate { degrees } if degrees == 90.0));
}

#[test]
fn an_object_edit_writes_the_same_flat_shape() {
    let edit = ObjectEdit { page: 1, index: 2, action: ObjectAction::Rotate { degrees: 45.0 } };
    assert_eq!(
        serde_json::to_value(&edit).expect("serialize"),
        serde_json::json!({ "page": 1, "index": 2, "action": "rotate", "degrees": 45.0 })
    );
}
