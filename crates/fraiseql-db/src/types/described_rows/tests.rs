use super::NumericType;

#[test]
fn a_type_modifier_decodes_to_its_precision_and_scale() {
    for (type_modifier, expected) in [
        (-1, NumericType::Unconstrained),
        (
            (20 << 16) + 2 + 4,
            NumericType::Declared {
                precision: 20,
                scale:     2,
            },
        ),
        (
            (6 << 16) + 3 + 4,
            NumericType::Declared {
                precision: 6,
                scale:     3,
            },
        ),
        (
            (10 << 16) + 4,
            NumericType::Declared {
                precision: 10,
                scale:     0,
            },
        ),
        // numeric(5, -2): the scale's 11 bits hold -2 as 0x7fe.
        (
            (5 << 16) + 0x7fe + 4,
            NumericType::Declared {
                precision: 5,
                scale:     -2,
            },
        ),
    ] {
        assert_eq!(NumericType::from_type_modifier(type_modifier), expected, "{type_modifier}");
    }
}
