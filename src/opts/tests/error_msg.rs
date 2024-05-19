use crate::keybindings::KeyCombos;
use crate::opts::Config;

macro_rules! snapshot_config_parse_error {
    ( $( ($test_name:ident, $config_text:ident) ),* $(,)? ) => {
        $(
            #[test]
            fn $test_name() {
                $crate::test_utils::log::init();

                let err = $crate::opts::Config::load_from_str($config_text).unwrap_err();

                ::insta::with_settings!({
                    description => $config_text,
                }, {
                    ::insta::assert_snapshot!(err);
                });
            }
        )*
    }
}

const UNKNOWN_THEME: &str = r#"light-theme.code-highlighter = "doesnt-exist""#;
const INVALID_THEME_TY: &str = "light-theme.code-highlighter = []";

const FIX_THIS_SUCKY_ERROR_MESSAGE: &str = r#"
[keybindings]
base = [
    ["ToBottom", { key = " ", mod = ["Ctrl", "shift"] }],
    ["ZoomOut", [{ key = " ", mod = ["ctrl", "shift"] }, "Enter"]],
]
"#;

snapshot_config_parse_error!(
    (unknown_theme, UNKNOWN_THEME),
    (invalid_theme_ty, INVALID_THEME_TY),
    // FIXME: vv
    (fix_this_sucky_error_message, FIX_THIS_SUCKY_ERROR_MESSAGE),
);

fn keycombo_conflict_from_config(s: &str) -> anyhow::Result<anyhow::Error> {
    let Config { keybindings, .. } = Config::load_from_str(s)?;
    let err = KeyCombos::new(keybindings).unwrap_err();
    Ok(err)
}

macro_rules! snapshot_keycombo_conflict_err {
    ( $( ($test_name:ident, $config_text:ident) ),* $(,)? ) => {
        $(
            #[test]
            fn $test_name() {
                $crate::test_utils::log::init();

                let err = keycombo_conflict_from_config($config_text).unwrap();

                ::insta::with_settings!({
                    description => $config_text,
                }, {
                    ::insta::assert_snapshot!(err);
                });
            }
        )*
    }
}

const BASIC_EQUALITY: &str = r#"
[keybindings]
base = [
    ["ToTop", "a"],
    ["ZoomReset", "a"],
]
"#;

const SPECIAL_PREFIX: &str = r#"
[keybindings]
base = [
    ["ToBottom", { key = "Space", mod = ["Ctrl", "Shift"] }],
    ["ZoomOut", [{ key = "Space", mod = ["Ctrl", "Shift"] }, "Enter", 39]],
]
"#;

snapshot_keycombo_conflict_err!(
    (basic_equality, BASIC_EQUALITY),
    (special_prefix, SPECIAL_PREFIX),
);
