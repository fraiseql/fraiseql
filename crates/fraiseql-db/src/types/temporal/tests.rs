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
        assert_eq!(Temporal::LocalDateTime.refusal(text), None, "{text}");
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
        assert!(
            Temporal::LocalDateTime.refusal(text).is_some_and(|w| w.contains("offset")),
            "{text}"
        );
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
        assert_eq!(Temporal::LocalDateTime.refusal(text), Some("is not one"), "{text}");
    }
}

#[test]
fn an_operand_is_checked_item_by_item_and_named() {
    assert!(Temporal::LocalDateTime.check_operand("at", &json!(null)).is_ok());
    assert!(
        Temporal::LocalDateTime
            .check_operand("at", &json!(["2026-07-01T12:00:00"]))
            .is_ok()
    );
    for operand in [
        json!(["2026-07-01T12:00:00", "2026-07-01T12:00:00Z"]),
        json!(20_260_701),
    ] {
        let err = Temporal::LocalDateTime
            .check_operand("at_lt", &operand)
            .expect_err("refused")
            .to_string();
        assert!(err.contains("`at_lt`") && err.contains("LocalDateTime"), "{err}");
        assert!(!err.contains("2026"), "the value is described, never quoted: {err}");
    }
}

#[test]
fn a_date_time_carries_an_offset_and_is_written_as_rfc_3339() {
    for text in [
        "2026-07-01T12:00:00Z",
        "2026-07-01t12:00:00z",
        "2026-07-01T12:00:00+02:00",
        "2026-07-01T12:00:00.5-08:00",
        "2026-07-01T12:00:00.123456789+00:00",
        "2024-02-29T23:59:59+23:59",
    ] {
        assert_eq!(Temporal::DateTime.refusal(text), None, "{text}");
    }
    for text in ["2026-07-01T12:00:00", "2026-07-01T12:00:00.5"] {
        assert!(
            Temporal::DateTime
                .refusal(text)
                .is_some_and(|w| w.contains("no time-zone offset")),
            "{text}"
        );
    }
    assert!(
        Temporal::DateTime
            .refusal("2026-07-01T12:00:00-00:00")
            .is_some_and(|w| w.contains("-00:00")),
    );
    for text in [
        "2026-07-01",
        "2026-07-01 12:00:00Z",
        "2026-07-01T12:00Z",
        "2026-07-01T12:00:00.Z",
        "2026-07-01T12:00:00.1234567890Z",
        "2026-07-01T24:00:00Z",
        "2026-07-01T23:59:60Z",
        "2026-02-30T12:00:00Z",
        "2026-07-01T12:00:00+02",
        "2026-07-01T12:00:00+0200",
        "2026-07-01T12:00:00+24:00",
        "2026-07-01T12:00:00+02:60",
        "2026-07-01T12:00:00+02:00:15",
        "2026-07-01T12:00:00 Europe/Paris",
        "20260701T120000Z",
        "now",
        "",
    ] {
        assert_eq!(Temporal::DateTime.refusal(text), Some("is not one"), "{text}");
    }
}

#[test]
fn a_date_is_a_real_calendar_date_and_nothing_after_it() {
    for text in ["2026-07-01", "2024-02-29", "0001-01-01"] {
        assert_eq!(Temporal::Date.refusal(text), None, "{text}");
    }
    for text in [
        "2025-02-29",
        "2026-13-01",
        "2026-7-1",
        "01/07/2026",
        "2026-07-01T00:00:00Z",
        "2026-07-01Z",
        "2026-07-01 ",
        "today",
    ] {
        assert_eq!(Temporal::Date.refusal(text), Some("is not one"), "{text}");
    }
}

#[test]
fn a_time_has_seconds_and_never_an_offset() {
    for text in ["09:00:00", "00:00:00", "23:59:59.123456789", "12:30:00.5"] {
        assert_eq!(Temporal::Time.refusal(text), None, "{text}");
    }
    for text in ["09:00:00+02:00", "09:00:00Z", "09:00:00.5-08:00"] {
        assert!(Temporal::Time.refusal(text).is_some_and(|w| w.contains("offset")), "{text}");
    }
    for text in [
        "09:00",
        "24:00:00",
        "23:60:00",
        "23:59:60",
        "9:00:00",
        "09:00:00.",
        "allballs",
    ] {
        assert_eq!(Temporal::Time.refusal(text), Some("is not one"), "{text}");
    }
}

#[test]
fn each_scalar_names_itself_and_its_shape() {
    for (scalar, says) in [
        (Temporal::DateTime, "takes a DateTime"),
        (Temporal::Date, "takes a Date (YYYY-MM-DD)"),
        (Temporal::Time, "takes a Time"),
    ] {
        let err = scalar.check_operand("at", &json!("x")).expect_err("refused").to_string();
        assert!(err.contains("`at`") && err.contains(says), "{err}");
    }
}
