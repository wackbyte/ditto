mod macros {
    macro_rules! assert_parses {
        ($config:expr) => {{
            $crate::tests::macros::assert_parses!($config, _)
        }};
        ($config:expr, $want:pat_param) => {{
            let result = toml::from_str::<$crate::Config>($config);
            assert!(matches!(result, Ok($want)), "{:#?}", result);
            result.unwrap()
        }};
    }
    pub(super) use assert_parses;

    macro_rules! assert_error {
        ($config:expr) => {{
            let result = toml::from_str::<$crate::Config>($config);
            assert!(matches!(result, Err(_)), "{:#?}", result);
            result.unwrap_err()
        }};
    }
    pub(super) use assert_error;
}

mod successes {
    use super::macros::assert_parses;
    use crate::{CodegenJsConfig, Config};

    #[test]
    fn it_parses_a_minimal_config() {
        assert_parses!(
            r#"
            name = "test" 
        "#
        );
        assert_parses!(
            r#"
            name = "test" 
            dependencies = []
            [package-set] 
        "#
        );
    }

    #[test]
    fn it_parses_ditto_version_requirements() {
        assert_parses!(
            r#"
            name = "test" 
            ditto-version = "1.0"
        "#,
            Config {
                required_ditto_version: Some(_),
                ..
            }
        );
        assert_parses!(
            r#"
            name = "test" 
            ditto-version = "^1.0"
        "#
        );
        assert_parses!(
            r#"
            name = "test" 
            ditto-version = "~1.0"
        "#
        );
    }

    #[test]
    fn it_parses_package_specs() {
        assert_parses!(
            r#"
            name = "test"
            dependencies = ["foo"]

            [package-set.packages] 
            foo = { path = "../test" }
        "#
        );
    }

    #[test]
    fn it_parses_targets() {
        assert_parses!(
            r#"
            name = "test" 
            targets = []
        "#
        );
        assert_parses!(
            r#"
            name = "test" 
            targets = ["nodejs", "web"]
        "#
        );
        // Duplicates are fine(?)
        assert_parses!(
            r#"
            name = "test" 
            targets = ["nodejs", "nodejs"]
        "#
        );
    }

    #[test]
    fn it_parses_js_package_json() {
        assert_parses!(
            r#"
            name = "test" 
            targets = []
            [codegen-js]
            package-json = { test = "2" }
        "#,
            Config {
                codegen_js_config: CodegenJsConfig {
                    package_json_additions: Some(_),
                    ..
                },
                ..
            }
        );
    }
}

mod errors {
    use super::macros::assert_error;

    #[test]
    fn it_errors_for_empty_input() {
        assert_error!("");
    }

    #[test]
    fn it_errors_for_invalid_targets() {
        assert_error!(
            r#"
            name = "test" 
            targets = ["not-real"]
        "#
        );
    }

    #[test]
    fn it_errors_for_bad_package_names() {
        assert_error!(
            r#"
            name = "NAH" 
        "#
        );
        assert_error!(
            r#"
            name = "test" 
            dependencies = ["NAH"]
        "#
        );
        assert_error!(
            r#"
            name = "test" 
            dependencies = ["test"]
            [package-set.packages]
            NAH = { path = "./not-real" }
        "#
        );
    }
}

#[snapshot_test::snapshot_lf(
    input = "golden-tests/parse-errors/(.*).toml",
    output = "golden-tests/parse-errors/${1}.error"
)]
fn golden(input: &str) -> String {
    let parse_error = crate::Config::parse("ditto.toml", input).unwrap_err();
    render_diagnostic(&parse_error)
}

fn render_diagnostic(diagnostic: &dyn miette::Diagnostic) -> String {
    let mut rendered = String::new();
    miette::GraphicalReportHandler::new()
        .with_theme(miette::GraphicalTheme {
            // Need to be explicit about this, because the `Default::default()`
            // is impure and can vary between environments, which is no good for testing
            characters: miette::ThemeCharacters::unicode(),
            styles: miette::ThemeStyles::none(),
        })
        .with_context_lines(3)
        .render_report(&mut rendered, diagnostic)
        .unwrap();
    rendered
}
