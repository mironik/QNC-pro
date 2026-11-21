use super::*;

    #[test]
    fn a_typed_number_is_written_and_a_word_is_not() {
        let mut settings = serde_json::json!({});
        assert!(set_number(&mut settings, "input.fps", "29,97", false));
        assert_eq!(settings["input"]["fps"], serde_json::json!(29.97));
        assert!(set_number(&mut settings, "input.width", "1920.4", true));
        assert_eq!(settings["input"]["width"], serde_json::json!(1920));
        assert!(!set_number(&mut settings, "input.width", "auto", true));
        let condition = Condition { path: "input.mode".into(), equals: "manual".into(), default: "auto".into() };
        assert!(!condition.holds(&settings));
    }

    #[test]
    fn a_length_is_typed_as_minutes_and_seconds_and_kept_in_seconds() {
        assert_eq!((parse_seconds("1:30"), parse_seconds("90"), parse_seconds("")), (Some(90), Some(90), Some(0)));
        assert_eq!((parse_seconds("1:75"), parse_seconds("-5"), parse_seconds("x")), (None, None, None));
        assert_eq!((minutes_seconds(90), minutes_seconds(0)), ("1:30".to_string(), String::new()));
    }

    #[test]
    fn a_schema_reads_from_its_contract() {
        let field: FieldSpec = serde_json::from_value(serde_json::json!({
            "kind": "integer", "label": "Width", "id": "w", "path": "input.width"
        }))
        .unwrap();
        assert_eq!((field.kind, field.options), (FieldKind::Integer, None));
    }
