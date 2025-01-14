#![doc = include_str!("../README.md")]
#![feature(box_patterns)]
#![warn(missing_docs)]

mod ast;
mod convert;
mod render;
mod ts;

pub use convert::Config;

/// Generate a JavaScript module from a ditto module.
pub fn codegen(config: &Config, module: ditto_ast::Module) -> String {
    render::render_module(convert::convert_module(config, module))
}

/// Generate a JavaScript module from a ditto module, with TypeScript declarations.
#[doc(hidden)]
pub fn codegen_with_dts(config: &Config, module: ditto_ast::Module) -> (String, String) {
    let dts = ts::generate_declarations(config, &module.module_name, &module.exports);
    let js = render::render_module(convert::convert_module(config, module));
    (js, dts)
}

#[cfg(test)]
mod tests {
    use crate as js;
    use ditto_ast as ast;
    use ditto_checker as checker;
    use ditto_cst as cst;

    #[snapshot_test::snapshot_lf(
        input = "golden-tests/javascript/(.*).ditto",
        output = "golden-tests/javascript/${1}.js"
    )]
    fn javascript(input: &str) -> String {
        let cst_module = cst::Module::parse(input).unwrap();
        let everything = mk_everything();
        let (ast_module, _warnings) = checker::check_module(&everything, cst_module).unwrap();
        prettier(&js::codegen(
            &js::Config {
                module_name_to_path: Box::new(module_name_to_path),
                foreign_module_path: "./foreign.js".into(),
            },
            ast_module,
        ))
    }

    #[snapshot_test::snapshot_lf(
        input = "golden-tests/typescript/(.*).ditto",
        output = "golden-tests/typescript/${1}.d.ts"
    )]
    fn typescript(input: &str) -> String {
        let cst_module = cst::Module::parse(input).unwrap();
        let everything = mk_everything();
        let (ast_module, _warnings) = checker::check_module(&everything, cst_module).unwrap();
        prettier(
            &js::codegen_with_dts(
                &js::Config {
                    module_name_to_path: Box::new(module_name_to_path),
                    foreign_module_path: "./foreign.js".into(),
                },
                ast_module,
            )
            .1,
        )
    }

    /// Use prettier to make sure the generated code is valid syntactically.
    fn prettier(text: &str) -> String {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };

        let mut child = Command::new("node")
            // NOTE: node_modules/.bin/prettier is a shell script on windows
            .arg("../../node_modules/prettier/bin-prettier.js")
            .arg("--parser")
            .arg("typescript")
            // NOTE: prettier defaults to `--end-of-line=lf`
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let child_stdin = child.stdin.as_mut().unwrap();
        child_stdin.write_all(text.as_bytes()).unwrap();
        // Close stdin to finish and avoid indefinite blocking
        drop(child_stdin);

        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }

    fn mk_everything() -> checker::Everything {
        let source = r#"
            module Data.Stuff exports (..);
            type Maybe(a) = Just(a) | Nothing;
            type Five = Five;
            five : Int = 5;
            five_string = "five" ;

            id = (a) -> a;
        "#;
        let cst_module = cst::Module::parse(source).unwrap();
        let (ast_module, _warnings) =
            checker::check_module(&checker::Everything::default(), cst_module).unwrap();
        let exports = ast_module.exports;

        checker::Everything {
            packages: std::collections::HashMap::from_iter([(
                ast::package_name!("test-stuff"),
                std::collections::HashMap::from_iter([(
                    ast::module_name!("Data", "Stuff"),
                    exports.clone(),
                )]),
            )]),
            modules: std::collections::HashMap::from_iter([(
                ast::module_name!("Data", "Stuff"),
                exports,
            )]),
        }
    }

    fn module_name_to_path((package_name, module_name): ast::FullyQualifiedModuleName) -> String {
        let module_path = module_name
            .0
            .into_iter()
            .map(|proper_name| proper_name.0)
            .collect::<Vec<_>>()
            .join(".");

        match package_name {
            None => module_path,
            Some(ast::PackageName(pkg)) => format!("{}/{}", pkg, module_path),
        }
    }
}
