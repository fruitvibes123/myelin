                                                                               
                                                               

use myelin::inference::ChatResponse;

#[test]
fn the_constructors_and_the_default_carry_no_finish_reason() {
    let done = ChatResponse::done("x");
    assert_eq!(done.content.as_deref(), Some("x"), "reached done()");
    assert_eq!(done.finish_reason, None, "done() carried a finish reason");

    let tool = ChatResponse::tool("read_file", serde_json::json!({ "path": "a" }));
    assert_eq!(tool.tool_calls.len(), 1, "reached tool()");
    assert_eq!(tool.finish_reason, None, "tool() carried a finish reason");

    let empty = ChatResponse::default();
    assert!(empty.content.is_none(), "reached default()");
    assert_eq!(
        empty.finish_reason, None,
        "default() carried a finish reason"
    );
}

const NOT_LENGTH: [&str; 7] = [
    "Length", "LENGTH", "lengthy", "length_", " length", "length ", "",
];

fn with_reason(reason: Option<&str>) -> ChatResponse {
    ChatResponse {
        content: Some("x".into()),
        tool_calls: Vec::new(),
        finish_reason: reason.map(str::to_string),
    }
}

#[test]
fn length_capped_is_equality_with_the_string_length() {
    let capped = with_reason(Some("length"));
    assert_eq!(
        capped.finish_reason.as_deref(),
        Some("length"),
        "reached the field for \"length\""
    );
    assert!(capped.length_capped(), "predicate at \"length\"");

    for r in NOT_LENGTH {
        let resp = with_reason(Some(r));
        assert_eq!(
            resp.finish_reason.as_deref(),
            Some(r),
            "reached the field for {r:?}"
        );
        assert!(!resp.length_capped(), "predicate at {r:?}");
    }

    let absent = with_reason(None);
    assert_eq!(absent.finish_reason, None, "reached the field for None");
    assert!(!absent.length_capped(), "predicate at None");
}
