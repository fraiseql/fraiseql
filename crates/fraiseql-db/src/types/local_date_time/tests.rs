use serde_json::json;

use super::*;

#[test]
fn a_local_date_time_in_every_spelled_form_is_one() {
    for text in [
        "2026-07-01T12:00:00",
        "2026-07-01t12:00:00",
        "2026-07-01T12:00:00.1",
        "2026-07-01T12:00:00.123456789",
        "2024-02-29T23:59:59",
        "0001-01-01T00:00:00",
    ] {
        assert_eq!(local_date_time_refusal(text), None, "{text}");
    }
}

#[test]
fn an_offset_or_a_z_is_named_as_an_offset() {
    for text in [
        "2026-07-01T12:00:00Z",
        "2026-07-01T12:00:00z",
        "2026-07-01T12:00:00+02:00",
        "2026-07-01T12:00:00.5-08:00",
    ] {
        assert!(local_date_time_refusal(text).is_some_and(|w| w.contains("offset")), "{text}");
    }
}

#[test]
fn a_string_of_another_shape_or_an_impossible_time_is_not_one() {
    for text in [
        "2026-07-01 12:00:00",
        "2026-07-01",
        "2026-07-01T12:00",
        "2026-07-01T12:00:00.",
        "2026-07-01T12:00:00.1234567890",
        "2026-07-01T12:00:00 ",
        "2026-13-01T12:00:00",
        "2025-02-29T12:00:00",
        "2026-07-01T24:00:00",
        "2026-07-01T12:60:00",
        "2026-07-01T12:00:60",
        "2026-07-01T12:00:00+02:00x",
        "tomorrow",
        "",
        "２026-07-01T12:00:00",
    ] {
        assert_eq!(local_date_time_refusal(text), Some("is not one"), "{text}");
    }
}

#[test]
fn an_operand_is_checked_item_by_item_and_named() {
    assert!(check_local_date_time_operand("at", &json!(null)).is_ok());
    assert!(check_local_date_time_operand("at", &json!(["2026-07-01T12:00:00"])).is_ok());
    for operand in [
        json!(["2026-07-01T12:00:00", "2026-07-01T12:00:00Z"]),
        json!(20_260_701),
    ] {
        let err = check_local_date_time_operand("at_lt", &operand)
            .expect_err("refused")
            .to_string();
        assert!(err.contains("`at_lt`") && err.contains("LocalDateTime"), "{err}");
        assert!(!err.contains("2026"), "the value is described, never quoted: {err}");
    }
}
