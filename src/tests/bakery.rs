use serde_json::json;

use crate::handlers::bakery::{
    Bakery, CursorParams, build_cursor_page, sanitized_cursor_page_size,
};

#[test]
fn cursor_params_accept_after_and_default_size() {
    let params: CursorParams =
        serde_json::from_value(json!({ "after": 12 })).expect("cursor params should deserialize");

    assert_eq!(params.after, Some(12));
    assert_eq!(params.size, None);
    assert_eq!(sanitized_cursor_page_size(params.size), 10);
}

#[test]
fn cursor_page_uses_extra_row_to_compute_next_cursor() {
    let bakeries = vec![
        Bakery { id: 1, name: "A".to_string(), profit_margin: 1.0 },
        Bakery { id: 2, name: "B".to_string(), profit_margin: 2.0 },
        Bakery { id: 3, name: "C".to_string(), profit_margin: 3.0 },
    ];

    let page = build_cursor_page(bakeries, Some(0), 2);

    assert_eq!(page.data.len(), 2);
    assert_eq!(page.after, Some(0));
    assert_eq!(page.size, 2);
    assert!(page.has_more);
    assert_eq!(page.next_cursor, Some(2));
}

#[test]
fn cursor_page_omits_next_cursor_when_results_are_exhausted() {
    let bakeries = vec![
        Bakery { id: 4, name: "D".to_string(), profit_margin: 4.0 },
        Bakery { id: 5, name: "E".to_string(), profit_margin: 5.0 },
    ];

    let page = build_cursor_page(bakeries, Some(3), 2);

    assert_eq!(page.data.len(), 2);
    assert_eq!(page.after, Some(3));
    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
}
