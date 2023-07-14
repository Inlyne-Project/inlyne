macro_rules! snapshot_config_parse_error {
    ( $( ($test_name:ident, $md_text:ident) ),* $(,)? ) => {
        $(
            #[test]
            fn $test_name() {
                let err = ::toml::from_str::<$crate::opts::Config>($md_text).unwrap_err();

                ::insta::with_settings!({
                    description => $md_text,
                }, {
                    ::insta::assert_display_snapshot!(err);
                });
            }
        )*
    }
}

const UNKNOWN_THEME: &str = r#"light-theme.code-highlighter = "doesnt-exist""#;
const INVALID_THEME_TY: &str = "light-theme.code-highlighter = []";

snapshot_config_parse_error!(
    (unknown_theme, UNKNOWN_THEME),
    (invalid_theme_ty, INVALID_THEME_TY),
);
