use super::*;

    #[test]
    fn test_setup_test_env() {
        setup_test_env();
    }

    #[test]
    fn test_create_temp_dir() {
        let temp_dir = create_temp_dir();
        assert!(temp_dir.path().exists());
    }
