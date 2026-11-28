use tylax::{
    typst_file_to_latex_with_diagnostics, typst_file_to_latex_with_options, typst_to_latex,
    typst_to_latex_with_diagnostics, typst_to_latex_with_options, T2LOptions,
};

const ISOLATED_ENV_GUARD: &str = "TYLAX_TEST_ISOLATED_TYPST_ROOT";

fn run_without_typst_root(test_name: &str, body: impl FnOnce()) {
    if std::env::var_os(ISOLATED_ENV_GUARD).is_none() {
        let exe = std::env::current_exe().unwrap();
        let output = std::process::Command::new(exe)
            .args(["--exact", test_name, "--nocapture"])
            .env(ISOLATED_ENV_GUARD, "1")
            .env_remove("TYPST_ROOT")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "isolated subprocess test `{test_name}` failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        return;
    }
    body();
}

#[test]
fn ast_path_ignores_shadowed_terms_item() {
    for source in [
        "#import \"custom-module.typ\" as terms\n#terms.item(\"A\", \"B\")",
        "#import \"terms.typ\"\n#terms.item(\"A\", \"B\")",
        "#import \"mod.typ\": terms\n#terms.item(\"A\", \"B\")",
        "#import \"mod.typ\": custom as terms\n#terms.item(\"A\", \"B\")",
        "#let terms = (item: (a, b) => [#a #b])\n#terms.item(\"A\", \"B\")",
    ] {
        let plain = typst_to_latex(source);
        assert!(!plain.contains(r"\begin{description}"), "{source}: {plain}");
        assert!(!plain.contains(r"\item["), "{source}: {plain}");
        assert!(!plain.contains("__tylax_shadowed_ctx"), "{source}: {plain}");
    }

    let wildcard = "#import \"mod.typ\": *\n#terms.item(\"A\", \"B\")";
    let wildcard_out = typst_to_latex(wildcard);
    assert_eq!(
        wildcard_out.matches(r"\begin{description}").count(),
        1,
        "{wildcard_out}"
    );
    assert!(wildcard_out.contains(r"\item[A] B"), "{wildcard_out}");

    let fallback = typst_to_latex_with_diagnostics(
        "#import \"custom-module.typ\" as terms\n#terms.item(\"A\", \"B\")",
        &T2LOptions::default(),
    );
    assert!(
        !fallback.output.contains(r"\begin{description}"),
        "{}",
        fallback.output
    );
    assert!(!fallback.output.contains(r"\item["), "{}", fallback.output);

    let scoped = "#[#import \"custom-module.typ\" as terms\n#terms.item(\"A\", \"B\")]\n#terms.item(\"C\", \"D\")";
    let scoped_out = typst_to_latex(scoped);
    assert_eq!(
        scoped_out.matches(r"\begin{description}").count(),
        1,
        "{scoped_out}"
    );
    assert!(scoped_out.contains(r"\item[C] D"), "{scoped_out}");
    assert!(!scoped_out.contains(r"\item[A]"), "{scoped_out}");
}

#[test]
fn file_path_detects_wildcard_exported_terms_shadowing() {
    run_without_typst_root(
        "file_path_detects_wildcard_exported_terms_shadowing",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_wildcard_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            let shadowed_src = "#import \"provider.typ\": *\n#terms.item(\"A\", \"B\")";
            let main_shadowed = dir.join("main_shadowed.typ");
            let ast_shadowed = typst_file_to_latex_with_options(
                shadowed_src,
                &main_shadowed,
                &T2LOptions::default(),
            );
            assert!(
                !ast_shadowed.contains(r"\begin{description}"),
                "{ast_shadowed}"
            );
            assert!(!ast_shadowed.contains(r"\item["), "{ast_shadowed}");

            let diag_shadowed = typst_file_to_latex_with_diagnostics(
                shadowed_src,
                &main_shadowed,
                &T2LOptions::default(),
            );
            assert!(
                !diag_shadowed.output.contains(r"\begin{description}"),
                "{}",
                diag_shadowed.output
            );
            assert!(
                !diag_shadowed.output.contains(r"\item["),
                "{}",
                diag_shadowed.output
            );

            let unshadowed_src = "#import \"helper.typ\": *\n#terms.item(\"A\", \"B\")";
            let main_unshadowed = dir.join("main_unshadowed.typ");
            for output in [
                typst_file_to_latex_with_options(
                    unshadowed_src,
                    &main_unshadowed,
                    &T2LOptions::default(),
                ),
                typst_file_to_latex_with_diagnostics(
                    unshadowed_src,
                    &main_unshadowed,
                    &T2LOptions::default(),
                )
                .output,
            ] {
                assert_eq!(
                    output.matches(r"\begin{description}").count(),
                    1,
                    "{output}"
                );
                assert!(output.contains(r"\item[A] B"), "{output}");
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn file_path_tracks_variable_wildcard_import_terms_shadowing() {
    run_without_typst_root(
        "file_path_tracks_variable_wildcard_import_terms_shadowing",
        || {
            let dir = std::env::temp_dir()
                .join(format!("tylax_pr69_var_wildcard_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            let shadowed_src =
                "#let path = \"provider.typ\"\n#import path: *\n#terms.item(\"A\", \"B\")";
            let main_shadowed = dir.join("main_var_shadowed.typ");
            for output in [
                typst_file_to_latex_with_options(
                    shadowed_src,
                    &main_shadowed,
                    &T2LOptions::default(),
                ),
                typst_file_to_latex_with_diagnostics(
                    shadowed_src,
                    &main_shadowed,
                    &T2LOptions::default(),
                )
                .output,
            ] {
                assert!(!output.contains(r"\begin{description}"), "{output}");
                assert!(!output.contains(r"\item["), "{output}");
            }

            let unshadowed_src =
                "#let path = \"helper.typ\"\n#import path: *\n#terms.item(\"A\", \"B\")";
            let main_unshadowed = dir.join("main_var_unshadowed.typ");
            for output in [
                typst_file_to_latex_with_options(
                    unshadowed_src,
                    &main_unshadowed,
                    &T2LOptions::default(),
                ),
                typst_file_to_latex_with_diagnostics(
                    unshadowed_src,
                    &main_unshadowed,
                    &T2LOptions::default(),
                )
                .output,
            ] {
                assert_eq!(
                    output.matches(r"\begin{description}").count(),
                    1,
                    "{output}"
                );
                assert!(output.contains(r"\item[A] B"), "{output}");
            }

            let scoped_src = "#let path = \"provider.typ\"\n#[#let path = \"helper.typ\"\n#import path: *\n#terms.item(\"Inner\", \"Desc\")]\n#import path: *\n#terms.item(\"Outer\", \"Desc\")";
            let main_scoped = dir.join("main_var_scoped.typ");
            let scoped_ast =
                typst_file_to_latex_with_options(scoped_src, &main_scoped, &T2LOptions::default());
            assert_eq!(
                scoped_ast.matches(r"\begin{description}").count(),
                1,
                "{scoped_ast}"
            );
            assert!(scoped_ast.contains(r"\item[Inner] Desc"), "{scoped_ast}");
            assert!(!scoped_ast.contains(r"\item[Outer]"), "{scoped_ast}");

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn file_path_ignores_untaken_branch_and_deferred_import_path_assignments() {
    run_without_typst_root(
        "file_path_ignores_untaken_branch_and_deferred_import_path_assignments",
        || {
            let dir = std::env::temp_dir()
                .join(format!("tylax_pr69_cfg_wildcard_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            let assert_shadowed = |name: &str, src: &str| {
                let path = dir.join(name);
                for output in [
                    typst_file_to_latex_with_options(src, &path, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default()).output,
                ] {
                    assert!(!output.contains(r"\begin{description}"), "{name}: {output}");
                    assert!(!output.contains(r"\item["), "{name}: {output}");
                }
            };

            let assert_builtin = |name: &str, src: &str| {
                let path = dir.join(name);
                for output in [
                    typst_file_to_latex_with_options(src, &path, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default()).output,
                ] {
                    assert_eq!(
                        output.matches(r"\begin{description}").count(),
                        1,
                        "{name}: {output}"
                    );
                    assert!(output.contains(r"\item[A] B"), "{name}: {output}");
                }
            };

            assert_shadowed(
        "if_false_keep_provider.typ",
        "#let path = \"provider.typ\"\n#if false { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_builtin(
        "if_false_keep_helper.typ",
        "#let path = \"helper.typ\"\n#if false { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "else_taken_provider.typ",
        "#let path = \"helper.typ\"\n#if false { path = \"helper.typ\" } else { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "static_flag_false.typ",
        "#let flag = false\n#let path = \"provider.typ\"\n#if flag { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "loops_and_closure_keep_provider.typ",
        "#let path = \"provider.typ\"\n#while false { path = \"helper.typ\" }\n#for x in () { path = \"helper.typ\" }\n#let f() = { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_builtin(
        "loops_and_closure_keep_helper.typ",
        "#let path = \"helper.typ\"\n#while false { path = \"provider.typ\" }\n#for x in () { path = \"provider.typ\" }\n#let f() = { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );

            assert_shadowed(
        "short_circuit_and_false.typ",
        "#let path = \"provider.typ\"\n#let _ = false and { path = \"helper.typ\"; true }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_builtin(
        "short_circuit_or_true.typ",
        "#let path = \"helper.typ\"\n#let _ = true or { path = \"provider.typ\"; false }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "short_circuit_and_true.typ",
        "#let path = \"helper.typ\"\n#let _ = true and { path = \"provider.typ\"; true }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "short_circuit_or_false.typ",
        "#let path = \"helper.typ\"\n#let _ = false or { path = \"provider.typ\"; true }\n#import path: *\n#terms.item([A], [B])",
    );

            assert_shadowed(
        "for_single_tuple.typ",
        "#let path = \"helper.typ\"\n#for i in (0,) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "for_range_one.typ",
        "#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "while_once_flag.typ",
        "#let done = false\n#let path = \"helper.typ\"\n#while not done { path = \"provider.typ\"; done = true }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "while_break.typ",
        "#let path = \"helper.typ\"\n#while true { path = \"provider.typ\"; break; path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "while_counter.typ",
        "#let i = 0\n#let path = \"helper.typ\"\n#while i < 1 { path = \"provider.typ\"; i += 1 }\n#import path: *\n#terms.item([A], [B])",
    );

            std::fs::write(
        dir.join("reexport_for.typ"),
        "#let path = \"helper.typ\"\n#for i in (0,) { path = \"provider.typ\" }\n#import path: *\n",
    )
    .unwrap();
            std::fs::write(
        dir.join("reexport_while.typ"),
        "#let done = false\n#let path = \"helper.typ\"\n#while not done { path = \"provider.typ\"; done = true }\n#import path: *\n",
    )
    .unwrap();
            assert_shadowed(
                "reexport_for_main.typ",
                "#import \"reexport_for.typ\": *\n#terms.item([A], [B])",
            );
            assert_shadowed(
                "reexport_while_main.typ",
                "#import \"reexport_while.typ\": *\n#terms.item([A], [B])",
            );

            // Shadowed `range` function must not be expanded as the builtin `range`.
            assert_builtin(
        "shadowed_range_fn_keep_helper.typ",
        "#let range(n) = ()\n#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "shadowed_range_fn_keep_provider.typ",
        "#let range(n) = ()\n#let path = \"provider.typ\"\n#for i in range(1) { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_builtin(
        "shadowed_range_arrow_keep_helper.typ",
        "#let range = (n) => ()\n#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "shadowed_range_arrow_keep_provider.typ",
        "#let range = (n) => ()\n#let path = \"provider.typ\"\n#for i in range(1) { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "helper_import_allows_builtin_range.typ",
        "#import \"helper.typ\": *\n#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            std::fs::write(dir.join("range_override.typ"), "#let range(n) = ()\n").unwrap();
            assert_builtin(
        "module_import_shadows_builtin_range.typ",
        "#import \"range_override.typ\": *\n#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );

            // Wildcard and explicit imports overwrite earlier static `range` closures.
            std::fs::write(dir.join("range_one.typ"), "#let range(n) = (0,)\n").unwrap();
            assert_shadowed(
        "wildcard_import_replaces_earlier_range_closure.typ",
        "#let range(n) = ()\n#import \"range_one.typ\": *\n#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "named_import_replaces_earlier_range_closure.typ",
        "#let range(n) = ()\n#import \"range_one.typ\": range\n#let path = \"helper.typ\"\n#for i in range(1) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "module_import_shadows_builtin_range_reverse.typ",
        "#import \"range_override.typ\": *\n#let path = \"provider.typ\"\n#for i in range(1) { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])",
    );

            // `while` updating on iteration 10 or exceeding the static iteration cap.
            assert_shadowed(
        "while_iter10_provider.typ",
        "#let i = 0\n#let path = \"helper.typ\"\n#while i < 12 { if i == 10 { path = \"provider.typ\" }; i += 1 }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_builtin(
        "while_iter10_helper.typ",
        "#let i = 0\n#let path = \"provider.typ\"\n#while i < 12 { if i == 10 { path = \"helper.typ\" }; i += 1 }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "while_over_cap_widens_helper_to_unknown_shadowed.typ",
        "#let i = 0\n#let path = \"helper.typ\"\n#while i < 80 { if i == 71 { path = \"provider.typ\" }; i += 1 }\n#import path: *\n#terms.item([A], [B])",
    );
            let while_80_provider_to_helper = "#let i = 0\n#let path = \"provider.typ\"\n#while i < 80 { if i == 70 { path = \"helper.typ\" }; i += 1 }\n#import path: *\n#terms.item([A], [B])";
            let while_80_path = dir.join("while_over_cap_widens_provider_to_unknown_shadowed.typ");
            let while_80_ast = typst_file_to_latex_with_options(
                while_80_provider_to_helper,
                &while_80_path,
                &T2LOptions::default(),
            );
            assert!(
                !while_80_ast.contains(r"\begin{description}"),
                "AST --no-eval cap should stay conservatively shadowed: {while_80_ast}"
            );
            assert!(
                !while_80_ast.contains(r"\item["),
                "AST --no-eval cap should stay conservatively shadowed: {while_80_ast}"
            );
            let while_80_eval = typst_file_to_latex_with_diagnostics(
                while_80_provider_to_helper,
                &while_80_path,
                &T2LOptions::default(),
            );
            assert!(
                while_80_eval.warnings.is_empty(),
                "expected 0 warnings for evaluated 80-iter loop: {:?}",
                while_80_eval.warnings
            );
            assert_eq!(
                while_80_eval.output.matches(r"\begin{description}").count(),
                1,
                "{}",
                while_80_eval.output
            );
            assert!(
                while_80_eval.output.contains(r"\item[A] B"),
                "{}",
                while_80_eval.output
            );
            assert_shadowed(
        "while_over_eval_cap_falls_back_to_shadowed.typ",
        "#let i = 0\n#let path = \"provider.typ\"\n#while i < 10005 { if i == 10002 { path = \"helper.typ\" }; i += 1 }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "for_over_cap_widens_helper_to_unknown_shadowed.typ",
        "#let path = \"helper.typ\"\n#for i in range(80) { if i == 71 { path = \"provider.typ\" } }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_builtin(
        "while_over_cap_without_path_write_stays_builtin.typ",
        "#let i = 0\n#let path = \"helper.typ\"\n#while i < 80 { i += 1 }\n#import path: *\n#terms.item([A], [B])",
    );
            std::fs::write(
        dir.join("reexport_while_iter10.typ"),
        "#let i = 0\n#let path = \"helper.typ\"\n#while i < 12 { if i == 10 { path = \"provider.typ\" }; i += 1 }\n#import path: *\n",
    )
    .unwrap();
            std::fs::write(
        dir.join("reexport_while_over_cap.typ"),
        "#let i = 0\n#let path = \"helper.typ\"\n#while i < 80 { if i == 71 { path = \"provider.typ\" }; i += 1 }\n#import path: *\n",
    )
    .unwrap();
            assert_shadowed(
                "reexport_while_iter10_main.typ",
                "#import \"reexport_while_iter10.typ\": *\n#terms.item([A], [B])",
            );
            assert_shadowed(
                "reexport_while_over_cap_main.typ",
                "#import \"reexport_while_over_cap.typ\": *\n#terms.item([A], [B])",
            );

            // String iteration uses extended grapheme clusters rather than Unicode scalar values.
            assert_shadowed(
        "string_grapheme_matches_cluster.typ",
        "#let path = \"helper.typ\"\n#for ch in \"e\\u{0301}\" { if ch == \"e\\u{0301}\" { path = \"provider.typ\" } }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "string_grapheme_does_not_split_cluster.typ",
        "#let path = \"provider.typ\"\n#for ch in \"e\\u{0301}\" { if ch == \"e\" { path = \"helper.typ\" } }\n#import path: *\n#terms.item([A], [B])",
    );
            assert_shadowed(
        "string_grapheme_var_matches_cluster.typ",
        "#let s = \"e\\u{0301}\"\n#let path = \"helper.typ\"\n#for ch in s { if ch == \"e\\u{0301}\" { path = \"provider.typ\" } }\n#import path: *\n#terms.item([A], [B])",
    );

            // Aliased named import `terms as custom` only binds `custom`, leaving builtin `terms` intact.
            assert_builtin(
                "aliased_terms_import_keeps_builtin.typ",
                "#import \"provider.typ\": terms as custom\n#terms.item([A], [B])",
            );
            std::fs::write(
                dir.join("reexport_aliased_terms.typ"),
                "#import \"provider.typ\": terms as custom\n",
            )
            .unwrap();
            assert_builtin(
                "reexport_aliased_terms_keeps_builtin.typ",
                "#import \"reexport_aliased_terms.typ\": *\n#terms.item([A], [B])",
            );
            assert_shadowed(
                "named_unaliased_terms_still_shadows.typ",
                "#import \"provider.typ\": terms\n#terms.item([A], [B])",
            );

            // Project-root-relative `/...` imports resolve against the project root.
            assert_shadowed(
                "root_relative_wildcard_shadows.typ",
                "#import \"/provider.typ\": *\n#terms.item([A], [B])",
            );
            assert_builtin(
                "root_relative_helper_keeps_builtin.typ",
                "#import \"/helper.typ\": *\n#terms.item([A], [B])",
            );
            assert_shadowed(
                "root_relative_var_wildcard_shadows.typ",
                "#let path = \"/provider.typ\"\n#import path: *\n#terms.item([A], [B])",
            );
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            std::fs::write(
                dir.join("sub").join("reexport_root.typ"),
                "#import \"/provider.typ\": *\n",
            )
            .unwrap();
            assert_shadowed(
                "subdir_root_relative_reexport_shadows.typ",
                "#import \"sub/reexport_root.typ\": *\n#terms.item([A], [B])",
            );

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn many_closure_definitions_do_not_blow_up_static_analysis() {
    let mut source = String::from("#let f0(x) = x\n");
    for idx in 1..64 {
        source.push_str(&format!("#let f{idx}(x) = f{}(x)\n", idx - 1));
    }
    source.push_str("#terms.item(\"A\", \"B\")");

    let output = typst_to_latex(&source);
    assert_eq!(
        output.matches(r"\begin{description}").count(),
        1,
        "{output}"
    );
    assert!(output.contains(r"\item[A] B"), "{output}");
}

#[test]
fn relative_imports_cannot_escape_project_root() {
    run_without_typst_root("relative_imports_cannot_escape_project_root", || {
        let outer =
            std::env::temp_dir().join(format!("tylax_pr69_root_escape_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&outer);
        let project = outer.join("project");
        let sub = project.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        std::fs::write(
            outer.join("outside.typ"),
            "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
        )
        .unwrap();
        std::fs::write(
            project.join("provider.typ"),
            "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
        )
        .unwrap();
        std::fs::write(
            sub.join("reexport_parent.typ"),
            "#import \"../provider.typ\": *\n",
        )
        .unwrap();
        std::fs::write(
            sub.join("reexport_escape.typ"),
            "#import \"../../outside.typ\": *\n",
        )
        .unwrap();
        std::fs::write(
            sub.join("reexport_root_escape.typ"),
            "#import \"/../outside.typ\": *\n",
        )
        .unwrap();

        let assert_builtin = |name: &str, src: &str| {
            let path = project.join(name);
            for output in [
                typst_file_to_latex_with_options(src, &path, &T2LOptions::default()),
                typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default()).output,
            ] {
                assert_eq!(
                    output.matches(r"\begin{description}").count(),
                    1,
                    "{name}: {output}"
                );
                assert!(output.contains(r"\item[A] B"), "{name}: {output}");
            }
        };

        let assert_shadowed = |name: &str, src: &str| {
            let path = project.join(name);
            for output in [
                typst_file_to_latex_with_options(src, &path, &T2LOptions::default()),
                typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default()).output,
            ] {
                assert!(!output.contains(r"\begin{description}"), "{name}: {output}");
                assert!(!output.contains(r"\item["), "{name}: {output}");
            }
        };

        assert_builtin(
            "direct_parent_escape.typ",
            "#import \"../outside.typ\": *\n#terms.item([A], [B])",
        );
        assert_builtin(
            "var_parent_escape.typ",
            "#let path = \"../outside.typ\"\n#import path: *\n#terms.item([A], [B])",
        );
        assert_builtin(
            "nested_parent_escape.typ",
            "#import \"sub/../../outside.typ\": *\n#terms.item([A], [B])",
        );
        assert_builtin(
            "root_parent_escape.typ",
            "#import \"/../outside.typ\": *\n#terms.item([A], [B])",
        );
        assert_builtin(
            "subdir_reexport_escape.typ",
            "#import \"sub/reexport_escape.typ\": *\n#terms.item([A], [B])",
        );
        assert_builtin(
            "subdir_reexport_root_escape.typ",
            "#import \"sub/reexport_root_escape.typ\": *\n#terms.item([A], [B])",
        );

        #[cfg(windows)]
        {
            assert_builtin(
                "windows_backslash_parent_escape.typ",
                "#import \"..\\\\outside.typ\": *\n#terms.item([A], [B])",
            );
            assert_builtin(
                "windows_backslash_root_escape.typ",
                "#import \"\\\\outside.typ\": *\n#terms.item([A], [B])",
            );
        }

        #[cfg(unix)]
        {
            let link = project.join("link_outside.typ");
            std::os::unix::fs::symlink(outer.join("outside.typ"), &link).unwrap();
            assert_builtin(
                "unix_symlink_escape.typ",
                "#import \"link_outside.typ\": *\n#terms.item([A], [B])",
            );
        }

        assert_shadowed(
            "subdir_valid_parent_reexport.typ",
            "#import \"sub/reexport_parent.typ\": *\n#terms.item([A], [B])",
        );
        assert_shadowed(
            "in_project_parent_traversal.typ",
            "#import \"sub/../provider.typ\": *\n#terms.item([A], [B])",
        );

        let _ = std::fs::remove_dir_all(&outer);
    });
}

#[cfg(any(windows, unix))]
fn create_dir_link(link: &std::path::Path, target: &std::path::Path) {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "failed to create junction {} -> {}",
            link.display(),
            target.display()
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).unwrap();
    }
}

#[cfg(any(windows, unix))]
fn remove_dir_link(link: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::fs::remove_dir(link);
    }
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(link);
    }
}

#[cfg(any(windows, unix))]
#[test]
fn directory_link_preserves_logical_module_dir_for_nested_imports() {
    run_without_typst_root(
        "directory_link_preserves_logical_module_dir_for_nested_imports",
        || {
            let outer =
                std::env::temp_dir().join(format!("tylax_pr69_dir_link_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&outer);
            let outside_dir = outer.join("outside_dir");
            let project = outer.join("project");
            let a_dir = project.join("a");
            let b_dir = project.join("b");
            let real_sub = b_dir.join("real_sub");
            std::fs::create_dir_all(&outside_dir).unwrap();
            std::fs::create_dir_all(&a_dir).unwrap();
            std::fs::create_dir_all(&real_sub).unwrap();

            std::fs::write(
                outside_dir.join("mod.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                project.join("outside.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                a_dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                b_dir.join("provider.typ"),
                "#let item(a, b) = [Helper #a and #b.]\n",
            )
            .unwrap();
            std::fs::write(real_sub.join("mod.typ"), "#import \"../provider.typ\": *\n").unwrap();
            std::fs::write(
                real_sub.join("escape_from_shallow.typ"),
                "#import \"../../outside.typ\": *\n",
            )
            .unwrap();

            let link_sub = a_dir.join("link_sub");
            let shallow_link = project.join("shallow_link");
            let escape_link = project.join("escape_link");
            create_dir_link(&link_sub, &real_sub);
            create_dir_link(&shallow_link, &real_sub);
            create_dir_link(&escape_link, &outside_dir);

            let assert_builtin = |name: &str, src: &str| {
                let path = project.join(name);
                for output in [
                    typst_file_to_latex_with_options(src, &path, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default()).output,
                ] {
                    assert_eq!(
                        output.matches(r"\begin{description}").count(),
                        1,
                        "{name}: {output}"
                    );
                    assert!(output.contains(r"\item[A] B"), "{name}: {output}");
                }
            };

            let assert_shadowed = |name: &str, src: &str| {
                let path = project.join(name);
                for output in [
                    typst_file_to_latex_with_options(src, &path, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default()).output,
                ] {
                    assert!(!output.contains(r"\begin{description}"), "{name}: {output}");
                    assert!(!output.contains(r"\item["), "{name}: {output}");
                }
            };

            assert_shadowed(
                "junction_nested_import_uses_logical_parent.typ",
                "#import \"a/link_sub/mod.typ\": *\n#terms.item([A], [B])",
            );
            assert_builtin(
                "direct_nested_import_uses_b_parent.typ",
                "#import \"b/real_sub/mod.typ\": *\n#terms.item([A], [B])",
            );
            assert_shadowed(
                "direct_depth2_stays_inside_project.typ",
                "#import \"b/real_sub/escape_from_shallow.typ\": *\n#terms.item([A], [B])",
            );
            assert_builtin(
                "shallow_link_depth1_escapes_project_root.typ",
                "#import \"shallow_link/escape_from_shallow.typ\": *\n#terms.item([A], [B])",
            );
            assert_builtin(
                "link_pointing_outside_project_root_is_blocked.typ",
                "#import \"escape_link/mod.typ\": *\n#terms.item([A], [B])",
            );

            remove_dir_link(&link_sub);
            remove_dir_link(&shallow_link);
            remove_dir_link(&escape_link);
            let _ = std::fs::remove_dir_all(&outer);
        },
    );
}

#[cfg(any(windows, unix))]
#[test]
fn entry_opened_via_directory_link_uses_canonical_dir_under_typst_root() {
    let outer = std::env::temp_dir().join(format!("tylax_pr69_entry_link_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outer);
    let project = outer.join("project");
    let a_dir = project.join("a");
    let b_dir = project.join("b");
    let real_sub = b_dir.join("real_sub");
    std::fs::create_dir_all(&a_dir).unwrap();
    std::fs::create_dir_all(&real_sub).unwrap();

    std::fs::write(
        a_dir.join("provider.typ"),
        "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
    )
    .unwrap();
    std::fs::write(
        b_dir.join("provider.typ"),
        "#let item(a, b) = [Helper #a and #b.]\n",
    )
    .unwrap();
    std::fs::write(
        a_dir.join("rev_provider.typ"),
        "#let item(a, b) = [Helper #a and #b.]\n",
    )
    .unwrap();
    std::fs::write(
        b_dir.join("rev_provider.typ"),
        "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
    )
    .unwrap();

    let real_entry = real_sub.join("entry.typ");
    let real_entry_rev = real_sub.join("entry_rev.typ");
    std::fs::write(
        &real_entry,
        "#import \"../provider.typ\": *\n#terms.item([A], [B])\n",
    )
    .unwrap();
    std::fs::write(
        &real_entry_rev,
        "#import \"../rev_provider.typ\": *\n#terms.item([A], [B])\n",
    )
    .unwrap();

    let link_sub = a_dir.join("link_sub");
    create_dir_link(&link_sub, &real_sub);
    let link_entry = link_sub.join("entry.typ");
    let link_entry_rev = link_sub.join("entry_rev.typ");

    let run_cli = |path: &std::path::Path, no_eval: bool| -> String {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_t2l"));
        cmd.env("TYPST_ROOT", &project);
        if no_eval {
            cmd.arg("--no-eval");
        }
        cmd.arg(path);
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "t2l failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    for path in [&link_entry, &real_entry] {
        for no_eval in [false, true] {
            let stdout = run_cli(path, no_eval);
            assert_eq!(
                stdout.matches(r"\begin{description}").count(),
                1,
                "{} (no_eval={no_eval}): {stdout}",
                path.display()
            );
            assert!(
                stdout.contains(r"\item[A] B"),
                "{} (no_eval={no_eval}): {stdout}",
                path.display()
            );
        }
    }

    for path in [&link_entry_rev, &real_entry_rev] {
        for no_eval in [false, true] {
            let stdout = run_cli(path, no_eval);
            assert!(
                !stdout.contains(r"\begin{description}"),
                "{} (no_eval={no_eval}): {stdout}",
                path.display()
            );
            assert!(
                !stdout.contains(r"\item["),
                "{} (no_eval={no_eval}): {stdout}",
                path.display()
            );
        }
    }

    remove_dir_link(&link_sub);
    let _ = std::fs::remove_dir_all(&outer);
}

#[test]
fn entry_outside_explicit_typst_root_does_not_read_sibling_modules() {
    let outer =
        std::env::temp_dir().join(format!("tylax_pr69_outside_root_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outer);
    let project = outer.join("project");
    let outside = outer.join("outside");
    let nonexistent_root = outer.join("nonexistent_root");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    std::fs::write(
        project.join("provider.typ"),
        "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
    )
    .unwrap();
    std::fs::write(
        outside.join("provider.typ"),
        "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
    )
    .unwrap();

    let main_sibling = outside.join("main_sibling.typ");
    let main_root_rel = outside.join("main_root_rel.typ");
    std::fs::write(
        &main_sibling,
        "#import \"provider.typ\": *\n#terms.item([A], [B])\n",
    )
    .unwrap();
    std::fs::write(
        &main_root_rel,
        "#import \"/provider.typ\": *\n#terms.item([A], [B])\n",
    )
    .unwrap();

    let run_cli =
        |root: Option<&std::path::Path>, path: &std::path::Path, no_eval: bool| -> String {
            let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_t2l"));
            match root {
                Some(root_dir) => {
                    cmd.env("TYPST_ROOT", root_dir);
                }
                None => {
                    cmd.env_remove("TYPST_ROOT");
                }
            }
            if no_eval {
                cmd.arg("--no-eval");
            }
            cmd.arg(path);
            let output = cmd.output().unwrap();
            assert!(
                output.status.success(),
                "t2l failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).into_owned()
        };

    for (root, path) in [
        (&project, &main_sibling),
        (&project, &main_root_rel),
        (&nonexistent_root, &main_sibling),
    ] {
        for no_eval in [false, true] {
            let stdout = run_cli(Some(root), path, no_eval);
            assert_eq!(
                stdout.matches(r"\begin{description}").count(),
                1,
                "{} with root {} (no_eval={no_eval}): {stdout}",
                path.display(),
                root.display()
            );
            assert!(
                stdout.contains(r"\item[A] B"),
                "{} with root {} (no_eval={no_eval}): {stdout}",
                path.display(),
                root.display()
            );
        }
    }

    for no_eval in [false, true] {
        let stdout = run_cli(None, &main_sibling, no_eval);
        assert!(
            !stdout.contains(r"\begin{description}"),
            "control without TYPST_ROOT (no_eval={no_eval}): {stdout}"
        );
        assert!(
            !stdout.contains(r"\item["),
            "control without TYPST_ROOT (no_eval={no_eval}): {stdout}"
        );
    }

    let _ = std::fs::remove_dir_all(&outer);
}

#[test]
fn non_regular_oversized_and_repeated_imports_are_safe() {
    run_without_typst_root(
        "non_regular_oversized_and_repeated_imports_are_safe",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_import_caps_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("subdir")).unwrap();
            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            let mut oversized = String::from("#let terms = (item: (a, b) => [Huge])\n");
            oversized.push_str(&"// padding\n".repeat(120_000));
            std::fs::write(dir.join("huge.typ"), oversized).unwrap();

            for (name, src) in [
                (
                    "dir_target.typ",
                    "#import \"subdir\": *\n#terms.item([A], [B])",
                ),
                (
                    "huge_target.typ",
                    "#import \"huge.typ\": *\n#terms.item([A], [B])",
                ),
            ] {
                let entry = dir.join(name);
                for output in [
                    typst_file_to_latex_with_options(src, &entry, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &entry, &T2LOptions::default())
                        .output,
                ] {
                    assert_eq!(
                        output.matches(r"\begin{description}").count(),
                        1,
                        "{output}"
                    );
                    assert!(output.contains(r"\item[A] B"), "{output}");
                }
            }

            let repeated_src =
                "#import \"provider.typ\": *\n#import \"provider.typ\": *\n#terms.item([A], [B])";
            let repeated_entry = dir.join("repeated.typ");
            for output in [
                typst_file_to_latex_with_options(
                    repeated_src,
                    &repeated_entry,
                    &T2LOptions::default(),
                ),
                typst_file_to_latex_with_diagnostics(
                    repeated_src,
                    &repeated_entry,
                    &T2LOptions::default(),
                )
                .output,
            ] {
                assert!(!output.contains(r"\begin{description}"), "{output}");
                assert!(!output.contains(r"\item["), "{output}");
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn chained_doubling_static_values_do_not_oom_ast_path() {
    let mut doubled_str = String::from("#let s = \"0123456789abcdef\"\n");
    for _ in 0..25 {
        doubled_str.push_str("#let s = s + s\n");
    }
    doubled_str.push_str("#terms.item([A], [B])\n");

    let mut doubled_arr = String::from("#let a = (1, 2, 3, 4)\n");
    for _ in 0..25 {
        doubled_arr.push_str("#let a = (a, a)\n");
    }
    doubled_arr.push_str("#terms.item([A], [B])\n");

    for src in [doubled_str, doubled_arr] {
        let out = typst_to_latex(&src);
        assert_eq!(out.matches(r"\begin{description}").count(), 1, "{out}");
        assert!(out.contains(r"\item[A] B"), "{out}");
    }
}

#[test]
fn dead_branch_loops_do_not_starve_live_path_resolution() {
    run_without_typst_root(
        "dead_branch_loops_do_not_starve_live_path_resolution",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_dead_budget_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            let dead_prefix = "#if false {\n  for i in range(64) {\n    for j in range(64) {\n      let x = i + j\n    }\n  }\n}\n";
            let shadowed_live = "#let path = \"helper.typ\"\n#for i in (0,) { path = \"provider.typ\" }\n#import path: *\n#terms.item([A], [B])";
            let shadowed_with_dead = format!("{dead_prefix}{shadowed_live}");
            for src in [shadowed_live, shadowed_with_dead.as_str()] {
                let entry = dir.join("shadowed.typ");
                for output in [
                    typst_file_to_latex_with_options(src, &entry, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &entry, &T2LOptions::default())
                        .output,
                ] {
                    assert!(!output.contains(r"\begin{description}"), "{output}");
                    assert!(!output.contains(r"\item["), "{output}");
                }
            }

            let builtin_live = "#let path = \"provider.typ\"\n#for i in (0,) { path = \"helper.typ\" }\n#import path: *\n#terms.item([A], [B])";
            let builtin_with_dead = format!("{dead_prefix}{builtin_live}");
            for src in [builtin_live, builtin_with_dead.as_str()] {
                let entry = dir.join("builtin.typ");
                for output in [
                    typst_file_to_latex_with_options(src, &entry, &T2LOptions::default()),
                    typst_file_to_latex_with_diagnostics(src, &entry, &T2LOptions::default())
                        .output,
                ] {
                    assert_eq!(
                        output.matches(r"\begin{description}").count(),
                        1,
                        "{output}"
                    );
                    assert!(output.contains(r"\item[A] B"), "{output}");
                }
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn cascading_wildcard_reexports_stay_bounded_and_track_shadowing() {
    run_without_typst_root(
        "cascading_wildcard_reexports_stay_bounded_and_track_shadowing",
        || {
            let dir = std::env::temp_dir()
                .join(format!("tylax_pr69_wildcard_cap_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            let mut provider = String::from("#let terms = (item: (a, b) => [Custom #a and #b.])\n");
            for i in 0..400 {
                provider.push_str(&format!("#let v{i} = \"val_{i}\"\n"));
            }
            std::fs::write(dir.join("big_provider.typ"), provider).unwrap();

            let mut main_src = String::new();
            for stage in 0..24 {
                let reexport = format!("stage_{stage}.typ");
                std::fs::write(dir.join(&reexport), "#import \"big_provider.typ\": *\n").unwrap();
                main_src.push_str(&format!("#import \"{reexport}\": *\n"));
            }
            main_src.push_str("#terms.item([A], [B])");

            let entry = dir.join("main.typ");
            for output in [
                typst_file_to_latex_with_options(&main_src, &entry, &T2LOptions::default()),
                typst_file_to_latex_with_diagnostics(&main_src, &entry, &T2LOptions::default())
                    .output,
            ] {
                assert!(!output.contains(r"\begin{description}"), "{output}");
                assert!(!output.contains(r"\item["), "{output}");
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn deferred_closure_and_dead_import_do_not_starve_live_imports() {
    run_without_typst_root(
        "deferred_closure_and_dead_import_do_not_starve_live_imports",
        || {
            let dir = std::env::temp_dir()
                .join(format!("tylax_pr69_closure_budget_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            for i in 0..6 {
                let mut filler = format!("#let tag = {i}\n");
                filler.push_str(&"// dead import padding\n".repeat(35_000));
                std::fs::write(dir.join(format!("dead_mod_{i}.typ")), filler).unwrap();
            }

            let prefix = "#let unused(d: for k in range(64) { k }) = {\n  for i in range(64) {\n    for j in range(64) {\n      let x = i + j\n    }\n  }\n}\n#if false {\n  import \"dead_mod_0.typ\": *\n  import \"dead_mod_1.typ\": *\n  import \"dead_mod_2.typ\": *\n  import \"dead_mod_3.typ\": *\n  import \"dead_mod_4.typ\": *\n  import \"dead_mod_5.typ\": *\n}\n";
            let shadowed_src = format!(
                "{prefix}#let path = \"helper.typ\"\n#for i in (0,) {{ path = \"provider.typ\" }}\n#import path: *\n#terms.item([A], [B])"
            );
            let builtin_src = format!(
                "{prefix}#let path = \"provider.typ\"\n#for i in (0,) {{ path = \"helper.typ\" }}\n#import path: *\n#terms.item([A], [B])"
            );

            let shadowed_entry = dir.join("shadowed.typ");
            for output in [
                typst_file_to_latex_with_options(
                    &shadowed_src,
                    &shadowed_entry,
                    &T2LOptions::default(),
                ),
                typst_file_to_latex_with_diagnostics(
                    &shadowed_src,
                    &shadowed_entry,
                    &T2LOptions::default(),
                )
                .output,
            ] {
                assert!(!output.contains(r"\begin{description}"), "{output}");
                assert!(!output.contains(r"\item["), "{output}");
            }

            let builtin_entry = dir.join("builtin.typ");
            for output in [
                typst_file_to_latex_with_options(
                    &builtin_src,
                    &builtin_entry,
                    &T2LOptions::default(),
                ),
                typst_file_to_latex_with_diagnostics(
                    &builtin_src,
                    &builtin_entry,
                    &T2LOptions::default(),
                )
                .output,
            ] {
                assert_eq!(
                    output.matches(r"\begin{description}").count(),
                    1,
                    "{output}"
                );
                assert!(output.contains(r"\item[A] B"), "{output}");
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn evaluated_file_conversion_expands_imported_custom_terms_and_module_functions_without_warnings() {
    run_without_typst_root(
        "evaluated_file_conversion_expands_imported_custom_terms_and_module_functions_without_warnings",
        || {
            let dir = std::env::temp_dir()
                .join(format!("tylax_pr69_eval_imports_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("terms.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            for (name, src) in [
                (
                    "wildcard_provider.typ",
                    "#import \"provider.typ\": *\n#terms.item(\"A\", \"B\")",
                ),
                (
                    "bare_terms_module.typ",
                    "#import \"terms.typ\"\n#terms.item(\"A\", \"B\")",
                ),
                (
                    "aliased_module_and_item.typ",
                    "#import \"provider.typ\" as custom_mod: terms\n#terms.item(\"A\", \"B\")",
                ),
            ] {
                let res = typst_file_to_latex_with_diagnostics(
                    src,
                    &dir.join(name),
                    &T2LOptions::default(),
                );
                assert!(
                    res.warnings.is_empty(),
                    "{name} unexpected warnings: {:?}",
                    res.warnings
                );
                assert!(res.output.contains("Custom A and B."), "{name}: {}", res.output);
                assert!(!res.output.contains(r"\begin{description}"), "{name}: {}", res.output);
                assert!(!res.output.contains(r"\item["), "{name}: {}", res.output);
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn cross_module_shadowing_is_generic_across_llap_builtins() {
    run_without_typst_root(
        "cross_module_shadowing_is_generic_across_llap_builtins",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_llap_mod_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            std::fs::write(dir.join("h_provider.typ"), "#let h(x) = [H]\n").unwrap();
            std::fs::write(
                dir.join("measure_provider.typ"),
                "#let measure(x) = (width: 0pt)\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();

            let ctx = "#context { h(-measure($x$).width); $x$ }";
            for (name, mod_file) in [
                ("shadowed_h.typ", "h_provider.typ"),
                ("shadowed_measure.typ", "measure_provider.typ"),
            ] {
                let src = format!("#import \"{mod_file}\": *\n${ctx}$");
                let entry = dir.join(name);
                let ast_out =
                    typst_file_to_latex_with_options(&src, &entry, &T2LOptions::default());
                let eval_res =
                    typst_file_to_latex_with_diagnostics(&src, &entry, &T2LOptions::default());
                for (mode, output) in [("ast", &ast_out), ("eval", &eval_res.output)] {
                    assert!(!output.contains(r"\llap"), "{name} [{mode}]: {output}");
                    assert!(
                        output.contains(&format!("% {ctx}\n")),
                        "{name} [{mode}]: {output}"
                    );
                }
            }

            let ok_src = format!("#import \"helper.typ\": *\n${ctx}$");
            let ok_entry = dir.join("unshadowed.typ");
            let ok_ast =
                typst_file_to_latex_with_options(&ok_src, &ok_entry, &T2LOptions::default());
            let ok_eval =
                typst_file_to_latex_with_diagnostics(&ok_src, &ok_entry, &T2LOptions::default());
            assert!(ok_ast.contains(r"\llap{$x$}"), "{ok_ast}");
            assert!(ok_eval.output.contains(r"\llap{$x$}"), "{}", ok_eval.output);
            assert!(ok_eval.warnings.is_empty(), "{:?}", ok_eval.warnings);

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn circular_imports_and_recursive_includes_terminate_cleanly_in_both_modes() {
    run_without_typst_root(
        "circular_imports_and_recursive_includes_terminate_cleanly_in_both_modes",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_cycles_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            // Mutual & self module imports
            std::fs::write(dir.join("a.typ"), "#import \"b.typ\": *\n#let from_a = 1\n").unwrap();
            std::fs::write(dir.join("b.typ"), "#import \"a.typ\": *\n#let from_b = 2\n").unwrap();
            std::fs::write(
                dir.join("self_imp.typ"),
                "#import \"self_imp.typ\": *\n#terms.item([A], [B])\n",
            )
            .unwrap();

            // Mutual & self includes
            std::fs::write(dir.join("inc_a.typ"), "A #include \"inc_b.typ\"\n").unwrap();
            std::fs::write(dir.join("inc_b.typ"), "B #include \"inc_a.typ\"\n").unwrap();
            std::fs::write(dir.join("self_inc.typ"), "S #include \"self_inc.typ\"\n").unwrap();

            for (file_name, src) in [
                (
                    "mutual_import_entry.typ",
                    "#import \"a.typ\": *\n#terms.item([A], [B])",
                ),
                (
                    "self_imp.typ",
                    "#import \"self_imp.typ\": *\n#terms.item([A], [B])",
                ),
                ("mutual_include_entry.typ", "#include \"inc_a.typ\"\nAfter"),
                ("self_inc.typ", "S #include \"self_inc.typ\"\nAfter"),
            ] {
                let path = dir.join(file_name);
                let _ast_out = typst_file_to_latex_with_options(src, &path, &T2LOptions::default());
                let eval_res =
                    typst_file_to_latex_with_diagnostics(src, &path, &T2LOptions::default());
                assert!(
                    !eval_res.warnings.is_empty(),
                    "{file_name} expected cycle warning"
                );
                assert!(
                    !eval_res.output.contains("__tylax_shadowed_ctx"),
                    "{file_name}: {}",
                    eval_res.output
                );
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn imported_modules_resolve_data_reads_relative_to_module_directory() {
    run_without_typst_root(
        "imported_modules_resolve_data_reads_relative_to_module_directory",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_rel_data_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let sub = dir.join("sub");
            std::fs::create_dir_all(&sub).unwrap();

            std::fs::write(dir.join("shared.json"), r#"{"name": "RootJson"}"#).unwrap();
            std::fs::write(dir.join("malformed.json"), "\"").unwrap();
            std::fs::write(sub.join("local.txt"), "LocalText").unwrap();
            std::fs::write(
                sub.join("data_mod.typ"),
                "#let msg = read(\"local.txt\") + \"-\" + json(\"../shared.json\").name\n#let load_local() = read(\"local.txt\") + \":\" + json(\"../shared.json\").name\n#let tri(n) = if n <= 0 { 0 } else { n + tri(n - 1) }\n",
            )
            .unwrap();
            std::fs::write(
                sub.join("section.typ"),
                "Child heading\n\n#image(\"diagram.png\")\n",
            )
            .unwrap();
            std::fs::write(
                sub.join("mod_methods.typ"),
                "#let len() = \"ModLen\"\n#let keys() = \"ModKeys\"\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("provider.typ"),
                "#let terms = (item: (a, b) => [Custom #a and #b.])\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("helper.typ"),
                "#let item(a, b) = [Custom #a and #b.]\n",
            )
            .unwrap();
            std::fs::write(sub.join("inner.typ"), "#let path = \"provider.typ\"\n").unwrap();
            std::fs::write(
                dir.join("outer.typ"),
                "#import \"sub/inner.typ\"\n#let path = \"helper.typ\"\n",
            )
            .unwrap();

            // F2 (lexical closure isolation + recursive self-reference)
            let src = concat!(
                "#import \"sub/data_mod.typ\": msg, load_local, tri as sum_to\n",
                "#let read(p) = \"HIJACK\"\n",
                "#let json(p) = (name: \"HIJACK\")\n",
                "#msg / #load_local() / #sum_to(4)\n",
            );
            let res = typst_file_to_latex_with_diagnostics(
                src,
                &dir.join("main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                res.warnings.is_empty(),
                "expected 0 warnings, got: {:?}",
                res.warnings
            );
            assert!(
                res.output.contains("LocalText-RootJson"),
                "top-level module read missing in output: {}",
                res.output
            );
            assert!(
                res.output.contains("LocalText:RootJson"),
                "exported closure module read leaked or failed: {}",
                res.output
            );
            assert!(
                !res.output.contains("HIJACK"),
                "caller read/json leaked into module closure: {}",
                res.output
            );
            assert!(
                res.output.contains("10"),
                "imported recursive closure tri(4)=10 missing: {}",
                res.output
            );

            // F1 (malformed JSON returns warning rather than panicking in default build)
            let malformed_res = typst_file_to_latex_with_diagnostics(
                "#json(\"malformed.json\")",
                &dir.join("bad_json_main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                !malformed_res.warnings.is_empty(),
                "expected fallback warning for malformed JSON, output: {}",
                malformed_res.output
            );

            // F3 (#include preserves child base directory for relative #image)
            let inc_res = typst_file_to_latex_with_diagnostics(
                "#include \"sub/section.typ\"",
                &dir.join("inc_main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                inc_res.warnings.is_empty(),
                "expected 0 warnings for #include + #image, got: {:?}",
                inc_res.warnings
            );
            assert!(
                inc_res
                    .output
                    .contains(r"\includegraphics{sub/diagram.png}"),
                "expected rebased child image path, got: {}",
                inc_res.output
            );

            // F4 (module methods like `len` and `keys` dispatch to module exports before dict methods)
            let mod_res = typst_file_to_latex_with_diagnostics(
                "#import \"sub/mod_methods.typ\"\n#mod_methods.len() / #mod_methods.keys() / #(a: 1).len()",
                &dir.join("mod_methods_main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                mod_res.warnings.is_empty(),
                "expected 0 warnings for module method dispatch, got: {:?}",
                mod_res.warnings
            );
            assert!(
                mod_res.output.contains("ModLen") && mod_res.output.contains("ModKeys"),
                "expected module len/keys calls, got: {}",
                mod_res.output
            );
            assert!(
                mod_res.output.contains("/ 1"),
                "ordinary dict .len() should still return 1, got: {}",
                mod_res.output
            );

            // F5 (dotted submodule import traverses nested module path instead of outer export)
            let dotted_src =
                "#import \"outer.typ\": inner.path\n#import path: *\n#terms.item([A], [B])";
            let dotted_entry = dir.join("dotted_main.typ");
            let dotted_ast =
                typst_file_to_latex_with_options(dotted_src, &dotted_entry, &T2LOptions::default());
            let dotted_eval = typst_file_to_latex_with_diagnostics(
                dotted_src,
                &dotted_entry,
                &T2LOptions::default(),
            );
            for (mode, output) in [("ast", &dotted_ast), ("eval", &dotted_eval.output)] {
                assert!(
                    !output.contains(r"\begin{description}"),
                    "dotted submodule import should shadow terms [{mode}]: {output}"
                );
                assert!(
                    !output.contains(r"\item["),
                    "dotted submodule import should shadow terms [{mode}]: {output}"
                );
            }

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn included_show_rules_json_escapes_and_nbsp_shorthands_match_typst_and_latex() {
    run_without_typst_root(
        "included_show_rules_json_escapes_and_nbsp_shorthands_match_typst_and_latex",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_p2_fixes_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            // 1. #include with child #show rule (plus parent/block/sequential scoping and #set non-leakage)
            std::fs::write(
                dir.join("child.typ"),
                "#show strong: it => [CHILD SHOW]\n*Original*\n",
            )
            .unwrap();
            let inc_res = typst_file_to_latex_with_diagnostics(
                "#include \"child.typ\"\n\n*MainAfter*",
                &dir.join("inc_show_main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                inc_res.warnings.is_empty(),
                "expected 0 warnings for #include #show, got: {:?}",
                inc_res.warnings
            );
            assert!(inc_res.output.contains("CHILD SHOW"), "{}", inc_res.output);
            assert!(
                inc_res.output.contains(r"\textbf{MainAfter}"),
                "child show rule must not leak to parent: {}",
                inc_res.output
            );
            assert!(
                !inc_res.output.contains("show strong"),
                "raw show rule leaked: {}",
                inc_res.output
            );
            assert!(
                !inc_res.output.contains(r"\textbf{Original}"),
                "child show rule did not transform child strong: {}",
                inc_res.output
            );

            let order_res = typst_file_to_latex_with_diagnostics(
                "#set text(size: 12pt)\n*Before*\n#[#show strong: it => [IN]\n*Inner*]\n*Middle*\n#show strong: it => [AFTER]\n*After*\n= MyTitle\n#show heading.where(level: 1): it => [H1: #it.body]\n= Sec\n== Sub",
                &dir.join("show_order_main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                order_res.warnings.is_empty(),
                "unexpected warnings: {:?}",
                order_res.warnings
            );
            assert!(
                !order_res.output.contains("set text"),
                "{}",
                order_res.output
            );
            assert!(
                !order_res.output.contains("show strong"),
                "{}",
                order_res.output
            );
            assert!(
                order_res.output.contains(r"\textbf{Before}"),
                "{}",
                order_res.output
            );
            assert!(order_res.output.contains("IN"), "{}", order_res.output);
            assert!(
                !order_res.output.contains(r"\textbf{Inner}"),
                "{}",
                order_res.output
            );
            assert!(
                order_res.output.contains(r"\textbf{Middle}"),
                "{}",
                order_res.output
            );
            assert!(order_res.output.contains("AFTER"), "{}", order_res.output);
            assert!(
                !order_res.output.contains(r"\textbf{After}"),
                "{}",
                order_res.output
            );
            assert!(
                order_res.output.contains(r"\section{") && order_res.output.contains("MyTitle"),
                "{}",
                order_res.output
            );
            assert!(order_res.output.contains("H1: Sec"), "{}", order_res.output);
            assert!(
                order_res.output.contains(r"\subsection{") && order_res.output.contains("Sub"),
                "{}",
                order_res.output
            );

            // 2. JSON with colons in keys + escaped backslashes under default features & --all-features
            std::fs::write(
                dir.join("colon_and_backslash.json"),
                r#"{"a:b":"VALUE","slash":"a\\b"}"#,
            )
            .unwrap();
            let json_res = typst_file_to_latex_with_diagnostics(
                "#let d = json(\"colon_and_backslash.json\")\n#d.at(\"a:b\") / #d.slash.len()",
                &dir.join("json_main.typ"),
                &T2LOptions::default(),
            );
            assert!(
                json_res.warnings.is_empty(),
                "expected 0 warnings for colon/backslash JSON, got: {:?}",
                json_res.warnings
            );
            assert!(
                json_res.output.contains("VALUE / 3"),
                "expected `VALUE / 3`, got: {}",
                json_res.output
            );

            // 3. Typst markup shorthands (`10~kg`, dashes, ellipsis, soft hyphen, minus) in both modes
            for (input, expected) in [
                ("10~kg", "10~kg"),
                ("*10~kg*", r"\textbf{10~kg}"),
                ("/ Mass: 10~kg", r"\item[Mass] 10~kg"),
                ("1--9", "1--9"),
                ("a---b", "a---b"),
                ("a...b", r"a\ldots{}b"),
                ("hy-?phen", r"hy\-phen"),
                ("val -5", "val $-$5"),
            ] {
                let ast_out = typst_to_latex(input);
                let eval_res = typst_to_latex_with_diagnostics(input, &T2LOptions::default());
                assert!(
                    eval_res.warnings.is_empty(),
                    "{input} warnings: {:?}",
                    eval_res.warnings
                );
                assert!(
                    ast_out.contains(expected),
                    "AST mode failed for `{input}`: got `{ast_out}`, expected `{expected}`"
                );
                assert!(
                    eval_res.output.contains(expected),
                    "eval mode failed for `{input}`: got `{}`, expected `{expected}`",
                    eval_res.output
                );
            }
            let literal_tilde = typst_to_latex(r#"#terms.item("Key", "A ~ B")"#);
            assert!(
                literal_tilde.contains(r"\item[Key] A \textasciitilde{} B"),
                "literal string '~' must stay \\textasciitilde{{}}: {literal_tilde}"
            );

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn show_regex_rule_chaining_container_selectors_set_rules_and_unicode_escape() {
    // 1. Regex `#show` partial replacement
    let rx_single =
        typst_to_latex_with_diagnostics("#show regex(\"bc\"): [X]\nabcde", &T2LOptions::default());
    assert!(rx_single.warnings.is_empty(), "{:?}", rx_single.warnings);
    assert_eq!(rx_single.output.trim(), "aXde");

    let rx_multi = typst_to_latex_with_diagnostics(
        "#show regex(\"b+\"): it => [(#it)]\nabbbcbb",
        &T2LOptions::default(),
    );
    assert!(rx_multi.warnings.is_empty(), "{:?}", rx_multi.warnings);
    assert_eq!(rx_multi.output.trim(), "a(bbb)c(bb)");

    let overlap = typst_to_latex_with_diagnostics(
        "#show \"ab\": [X]\n#show \"bc\": [Y]\nabc",
        &T2LOptions::default(),
    );
    assert_eq!(overlap.output.trim(), "Xc");

    let tie = typst_to_latex_with_diagnostics(
        "#show \"abc\": [LONG]\n#show \"ab\": [SHORT]\nabc",
        &T2LOptions::default(),
    );
    assert_eq!(tie.output.trim(), "SHORTc");

    // Invalid empty textual selectors warn cleanly without hanging or panicking.
    let empty_rx =
        typst_to_latex_with_diagnostics("#show regex(\"b*\"): [X]\nac", &T2LOptions::default());
    assert!(!empty_rx.warnings.is_empty());
    assert_eq!(empty_rx.output.trim(), "ac");

    let empty_str = typst_to_latex_with_diagnostics("#show \"\": [X]\nab", &T2LOptions::default());
    assert!(!empty_str.warnings.is_empty());
    assert_eq!(empty_str.output.trim(), "ab");

    // 2. Rule composition, tie-breaking, and dynamic text
    for src in [
        "#show \"cat\": [dog]\n#show \"dog\": [bird]\ncat",
        "#show \"dog\": [bird]\n#show \"cat\": [dog]\ncat",
    ] {
        let out = typst_to_latex_with_diagnostics(src, &T2LOptions::default());
        assert!(out.warnings.is_empty(), "{src}: {:?}", out.warnings);
        assert_eq!(out.output.trim(), "bird", "{src}");
    }
    for src in [
        "#show \"cat\": [dog]\n#(\"cat\")",
        "#let s = \"cat\"\n#show \"cat\": [dog]\n#s",
        "#show \"cat\": [dog]\nca#(\"t\")",
        "#{ show \"cat\": [dog]; \"cat\" }",
    ] {
        let out = typst_to_latex_with_diagnostics(src, &T2LOptions::default());
        assert!(out.warnings.is_empty(), "{src}: {:?}", out.warnings);
        assert_eq!(out.output.trim(), "dog", "{src}");
    }
    let len_preserved = typst_to_latex_with_diagnostics(
        "#show \"cat\": [dog]\n#let s = \"cat\"\n#(s.len())",
        &T2LOptions::default(),
    );
    assert_eq!(len_preserved.output.trim(), "3");

    let elem_chain = typst_to_latex_with_diagnostics(
        "#show strong: it => emph(it.body)\n#show strong: it => [S:#it]\n*hi*",
        &T2LOptions::default(),
    );
    assert_eq!(elem_chain.output.trim(), r"S:\textit{hi}");

    let heading_chain = typst_to_latex_with_diagnostics(
        "#show heading: it => if it.level == 1 { heading(level: 2, it.body) } else { strong(it.body) }\n= Hi",
        &T2LOptions::default(),
    );
    assert_eq!(heading_chain.output.trim(), r"\textbf{Hi}");

    // 3. Container vs `.item` selectors (`terms`, `list`, `enum`)
    for terms_src in [
        "#show terms: [GLOSSARY]\n/ A: 1\n/ B: 2",
        "#show terms: [GLOSSARY]\n/ A: 1\n\n/ B: 2",
    ] {
        let out = typst_to_latex_with_diagnostics(terms_src, &T2LOptions::default());
        assert!(out.warnings.is_empty(), "{terms_src}: {:?}", out.warnings);
        assert_eq!(out.output.trim(), "GLOSSARY", "{terms_src}");
    }
    let terms_item = typst_to_latex_with_diagnostics(
        "#show terms.item: it => [#it.term=#it.description;]\n/ A: 1\n/ B: 2",
        &T2LOptions::default(),
    );
    assert_eq!(terms_item.output.trim(), "A=1 B=2");

    for list_src in [
        "#show list: [ONELIST]\n- a\n- b",
        "#show list: [ONELIST]\n#for x in (\"a\", \"b\") [- #x]",
    ] {
        let out = typst_to_latex_with_diagnostics(list_src, &T2LOptions::default());
        assert_eq!(out.output.trim(), "ONELIST", "{list_src}");
    }
    let list_item = typst_to_latex_with_diagnostics(
        "#show list.item: it => [(#it.body)]\n- a\n- b",
        &T2LOptions::default(),
    );
    assert_eq!(list_item.output.trim(), "(a) (b)");

    let enum_container =
        typst_to_latex_with_diagnostics("#show enum: [ONEENUM]\n+ a\n+ b", &T2LOptions::default());
    assert_eq!(enum_container.output.trim(), "ONEENUM");

    let enum_item = typst_to_latex_with_diagnostics(
        "#show enum.item: it => [(#it.body)]\n+ a\n+ b",
        &T2LOptions::default(),
    );
    assert_eq!(enum_item.output.trim(), "(a) (b)");

    // 4. Scoped `#set` rules (`list`, `enum`, `heading`) in BOTH modes
    let set_list_src = "#set list(marker: [Q])\n- one\n- two";
    for out in [
        typst_to_latex(set_list_src),
        typst_to_latex_with_diagnostics(set_list_src, &T2LOptions::default()).output,
    ] {
        assert!(out.contains(r"\begin{itemize}"), "{out}");
        assert!(out.contains(r"\item[Q] one"), "{out}");
        assert!(out.contains(r"\item[Q] two"), "{out}");
    }

    for scoped_list_src in [
        "#[#set list(marker: [Q])\n- inside]\n- outside",
        "#let q = [Q]\n#[#set list(marker: q)\n- inside]\n- outside",
    ] {
        for out in [
            typst_to_latex(scoped_list_src),
            typst_to_latex_with_diagnostics(scoped_list_src, &T2LOptions::default()).output,
        ] {
            assert!(out.contains(r"\item[Q] inside"), "{scoped_list_src}: {out}");
            assert!(out.contains(r"\item outside"), "{scoped_list_src}: {out}");
            assert!(
                !out.contains(r"\item[Q] outside"),
                "{scoped_list_src}: {out}"
            );
        }
    }

    let nested_cycle_src = "#set list(marker: ([A], [B]))\n- outer\n  - inner";
    for out in [
        typst_to_latex(nested_cycle_src),
        typst_to_latex_with_diagnostics(nested_cycle_src, &T2LOptions::default()).output,
    ] {
        assert!(out.contains(r"\item[A] outer"), "{out}");
        assert!(out.contains(r"\item[B] inner"), "{out}");
    }

    let closure_marker_src = "#set list(marker: n => [M#n])\n- zero\n  - one";
    let closure_out = typst_to_latex_with_diagnostics(closure_marker_src, &T2LOptions::default());
    assert!(
        closure_out.warnings.is_empty(),
        "{:?}",
        closure_out.warnings
    );
    assert!(
        closure_out.output.contains(r"\item[M0] zero"),
        "{}",
        closure_out.output
    );
    assert!(
        closure_out.output.contains(r"\item[M1] one"),
        "{}",
        closure_out.output
    );

    let set_enum_src = "#set enum(start: 3)\n+ one\n+ two";
    for out in [
        typst_to_latex(set_enum_src),
        typst_to_latex_with_diagnostics(set_enum_src, &T2LOptions::default()).output,
    ] {
        assert!(out.contains(r"\begin{enumerate}"), "{out}");
        assert!(out.contains(r"\setcounter{enumi}{2}"), "{out}");
    }

    let set_heading_src = "#set heading(numbering: none)\n= Unnumbered";
    for out in [
        typst_to_latex(set_heading_src),
        typst_to_latex_with_diagnostics(set_heading_src, &T2LOptions::default()).output,
    ] {
        assert!(out.contains(r"\section*{"), "{out}");
        assert!(out.contains("Unnumbered"), "{out}");
    }

    let set_heading_numbered_src = "#set heading(numbering: \"1.\")\n= Numbered";
    for out in [
        typst_to_latex(set_heading_numbered_src),
        typst_to_latex_with_diagnostics(set_heading_numbered_src, &T2LOptions::default()).output,
    ] {
        assert!(out.contains(r"\section{"), "{out}");
        assert!(!out.contains(r"\section*{"), "{out}");
    }

    let scoped_heading_src = "#[#set heading(numbering: none)\n= Inside]\n= Outside";
    for out in [
        typst_to_latex(scoped_heading_src),
        typst_to_latex_with_diagnostics(scoped_heading_src, &T2LOptions::default()).output,
    ] {
        assert!(
            out.contains(r"\section*{Inside}") || out.contains(r"\section*{ Inside}"),
            "{out}"
        );
        assert!(
            out.contains(r"\section{Outside}") || out.contains(r"\section{ Outside}"),
            "{out}"
        );
    }

    // 5. Explicit Unicode escape `\u{2212}` in both modes & math
    let unicode_minus_src = r#"val \u{2212}5"#;
    for out in [
        typst_to_latex(unicode_minus_src),
        typst_to_latex_with_diagnostics(unicode_minus_src, &T2LOptions::default()).output,
    ] {
        assert_eq!(out.trim(), "val $-$5");
        assert!(!out.contains('\u{2212}'), "raw U+2212 leaked: {out}");
    }

    let math_minus = typst_to_latex_with_options(r#"x \u{2212} 5"#, &T2LOptions::math_only());
    assert!(math_minus.contains("x - 5"), "{math_minus}");
    assert!(!math_minus.contains('\u{2212}'), "{math_minus}");
}

#[test]
fn show_label_selector_replaces_labeled_elements_and_preserves_unmatched_labels() {
    let basic = "= Original <topic>\n#show <next>: [REPLACEMENT]\n= Next <next>";
    let res = typst_to_latex_with_diagnostics(basic, &T2LOptions::default());
    assert!(res.warnings.is_empty(), "warnings: {:?}", res.warnings);
    assert!(res.output.contains(r"\section{"), "{}", res.output);
    assert!(res.output.contains("Original"), "{}", res.output);
    assert!(res.output.contains(r"\label{topic}"), "{}", res.output);
    assert!(res.output.contains("REPLACEMENT"), "{}", res.output);
    assert!(!res.output.contains("Next"), "{}", res.output);
    assert!(!res.output.contains(r"\label{next}"), "{}", res.output);

    let with_ref = "See @topic.\n\n= Original <topic>\n#show <next>: [REPLACEMENT]\n= Next <next>";
    let ref_res = typst_to_latex_with_diagnostics(with_ref, &T2LOptions::default());
    assert!(
        ref_res.warnings.is_empty(),
        "warnings: {:?}",
        ref_res.warnings
    );
    assert!(
        ref_res.output.contains(r"Section~\ref{topic}"),
        "{}",
        ref_res.output
    );
    assert!(ref_res.output.contains("REPLACEMENT"), "{}", ref_res.output);
    assert!(!ref_res.output.contains("Next"), "{}", ref_res.output);

    let via_var = "#let lbl = <next>\n#show lbl: [VAR]\n= Next <next>";
    let var_res = typst_to_latex_with_diagnostics(via_var, &T2LOptions::default());
    assert_eq!(var_res.output.trim(), "VAR");

    let via_fn = "#show label(\"next\"): [FN]\n= Next <next>";
    let fn_res = typst_to_latex_with_diagnostics(via_fn, &T2LOptions::default());
    assert_eq!(fn_res.output.trim(), "FN");

    let field_closure = "#show <next>: it => [L#it.level:#it.body]\n== Sub <next>";
    let field_res = typst_to_latex_with_diagnostics(field_closure, &T2LOptions::default());
    assert_eq!(field_res.output.trim(), "L2:Sub");

    let bold_labeled = "#show <b1>: it => [BOLD:#it.body]\nhello *world* <b1> end";
    let bold_res = typst_to_latex_with_diagnostics(bold_labeled, &T2LOptions::default());
    assert_eq!(bold_res.output.trim(), "hello BOLD:world end");

    let with_set = "#set heading(numbering: none)\n#show <next>: [REPL]\n= First\n= Second <next>";
    let set_res = typst_to_latex_with_diagnostics(with_set, &T2LOptions::default());
    assert!(set_res.output.contains(r"\section*{"), "{}", set_res.output);
    assert!(set_res.output.contains("First"), "{}", set_res.output);
    assert!(set_res.output.contains("REPL"), "{}", set_res.output);
    assert!(!set_res.output.contains("Second"), "{}", set_res.output);

    let or_sel = "#show selector(<a1>).or(<a2>): [OR]\n= First <a1>\n= Second <a2>";
    let or_res = typst_to_latex_with_diagnostics(or_sel, &T2LOptions::default());
    assert_eq!(or_res.output.matches("OR").count(), 2, "{}", or_res.output);
    assert!(
        or_res.warnings.is_empty(),
        "warnings: {:?}",
        or_res.warnings
    );
}

#[test]
fn show_replacement_preserves_numbered_cross_reference_anchors_and_counters() {
    let basic = "#set heading(numbering: \"1.\")\n#show <x>: [REPLACEMENT]\n= First\n= Second <x>\n= Third\nSee @x.";
    let res = typst_to_latex_with_diagnostics(basic, &T2LOptions::default());
    assert!(res.warnings.is_empty(), "warnings: {:?}", res.warnings);
    assert!(
        res.output
            .contains(r"\refstepcounter{section}\label{x}REPLACEMENT"),
        "{}",
        res.output
    );
    assert!(
        res.output.contains(r"See Section~\ref{x}."),
        "{}",
        res.output
    );
    assert_eq!(
        res.output.matches(r"\label{x}").count(),
        1,
        "{}",
        res.output
    );

    let sub = "#set heading(numbering: \"1.1\")\n#show <s1>: [SUBREPL]\n= One\n== SubA <s1>\n== SubB\nSee @s1.";
    let sub_res = typst_to_latex_with_diagnostics(sub, &T2LOptions::default());
    assert!(
        sub_res
            .output
            .contains(r"\refstepcounter{subsection}\label{s1}SUBREPL"),
        "{}",
        sub_res.output
    );
    assert!(
        sub_res.output.contains(r"See Section~\ref{s1}."),
        "{}",
        sub_res.output
    );

    let closure = "#set heading(numbering: \"1.\")\n#show <x>: it => [CUSTOM: #it.body]\n= First <x>\n= Second\nSee @x.";
    let closure_res = typst_to_latex_with_diagnostics(closure, &T2LOptions::default());
    assert!(
        closure_res
            .output
            .contains(r"\refstepcounter{section}\label{x}CUSTOM: First"),
        "{}",
        closure_res.output
    );
    assert!(
        closure_res.output.contains(r"See Section~\ref{x}."),
        "{}",
        closure_res.output
    );

    let where_sel =
        "#set heading(numbering: \"1.\")\n#show heading.where(level: 1): [HREPL]\n= First <x>\n= Second\nSee @x.";
    let where_res = typst_to_latex_with_diagnostics(where_sel, &T2LOptions::default());
    assert!(
        where_res
            .output
            .contains(r"\refstepcounter{section}\label{x}HREPL"),
        "{}",
        where_res.output
    );
    assert!(
        where_res.output.contains(r"See Section~\ref{x}."),
        "{}",
        where_res.output
    );

    let none_repl =
        "#set heading(numbering: \"1.\")\n#show <x>: none\n= First <x>\n= Second\nSee @x.";
    let none_res = typst_to_latex_with_diagnostics(none_repl, &T2LOptions::default());
    assert!(
        none_res
            .output
            .contains(r"\refstepcounter{section}\label{x}"),
        "{}",
        none_res.output
    );
    assert!(
        none_res.output.contains(r"See Section~\ref{x}."),
        "{}",
        none_res.output
    );

    let nested = "#set heading(numbering: \"1.\")\n#show <x>: [BLKREPL]\n#block[= First <x>]\n= Second\nSee @x.";
    let nested_res = typst_to_latex_with_diagnostics(nested, &T2LOptions::default());
    assert!(
        nested_res
            .output
            .contains(r"\refstepcounter{section}\label{x}BLKREPL"),
        "{}",
        nested_res.output
    );
    assert!(
        nested_res.output.contains(r"See Section~\ref{x}."),
        "{}",
        nested_res.output
    );

    let kept =
        "#set heading(numbering: \"1.\")\n#show <x>: it => [Prefix: #it]\n= First <x>\nSee @x.";
    let kept_res = typst_to_latex_with_diagnostics(kept, &T2LOptions::default());
    assert!(
        !kept_res.output.contains(r"\refstepcounter"),
        "{}",
        kept_res.output
    );
    assert_eq!(
        kept_res.output.matches(r"\label{x}").count(),
        1,
        "{}",
        kept_res.output
    );
    assert!(
        kept_res.output.contains(r"See Section~\ref{x}."),
        "{}",
        kept_res.output
    );

    let fig = "#show <f1>: [FIGREPL]\n#figure([FigBody], caption: [Cap]) <f1>\nSee @f1.";
    let fig_res = typst_to_latex_with_diagnostics(fig, &T2LOptions::default());
    assert!(
        fig_res
            .output
            .contains(r"\refstepcounter{figure}\label{f1}FIGREPL"),
        "{}",
        fig_res.output
    );
    assert!(
        fig_res.output.contains(r"See Figure~\ref{f1}."),
        "{}",
        fig_res.output
    );

    let tab =
        "#show <t1>: [TABREPL]\n#figure(table(columns: 2)[A][B], caption: [Cap]) <t1>\nSee @t1.";
    let tab_res = typst_to_latex_with_diagnostics(tab, &T2LOptions::default());
    assert!(
        tab_res
            .output
            .contains(r"\refstepcounter{table}\label{t1}TABREPL"),
        "{}",
        tab_res.output
    );
    assert!(
        tab_res.output.contains(r"See Table~\ref{t1}."),
        "{}",
        tab_res.output
    );

    let unnum = "#set heading(numbering: none)\n#show <next>: [REPL]\n= First\n= Second <next>";
    let unnum_res = typst_to_latex_with_diagnostics(unnum, &T2LOptions::default());
    assert!(
        !unnum_res.output.contains(r"\refstepcounter"),
        "{}",
        unnum_res.output
    );
    assert!(
        !unnum_res.output.contains(r"\label{next}"),
        "{}",
        unnum_res.output
    );
}

#[test]
fn labeled_items_group_variadic_or_and_func_equality_in_t2l() {
    for (src, group_word) in [
        (
            "#show list: [LGROUP]\n#list.item[One] <x>\n#list.item[Two]",
            "LGROUP",
        ),
        (
            "#show enum: [EGROUP]\n#enum.item[One] <x>\n#enum.item[Two]",
            "EGROUP",
        ),
        (
            "#show terms: [TGROUP]\n#terms.item[A][B] <x>\n#terms.item[C][D]",
            "TGROUP",
        ),
    ] {
        let res = typst_to_latex_with_diagnostics(src, &T2LOptions::default());
        assert_eq!(res.output.trim(), group_word, "{src} -> {}", res.output);
    }

    let custom_list = typst_to_latex_with_diagnostics(
        "#set list(marker: [Q])\n#list.item[One] <x>\n#list.item[Two]",
        &T2LOptions::default(),
    );
    assert_eq!(
        custom_list.output.matches(r"\begin{itemize}").count(),
        1,
        "{}",
        custom_list.output
    );
    assert_eq!(
        custom_list.output.matches(r"\item[Q]").count(),
        2,
        "{}",
        custom_list.output
    );
    assert!(
        custom_list.output.contains(r"One \label{x}"),
        "{}",
        custom_list.output
    );

    let custom_enum = typst_to_latex_with_diagnostics(
        "#set enum(start: 3)\n#enum.item[One] <x>\n#enum.item[Two]",
        &T2LOptions::default(),
    );
    assert_eq!(
        custom_enum.output.matches(r"\begin{enumerate}").count(),
        1,
        "{}",
        custom_enum.output
    );
    assert_eq!(
        custom_enum.output.matches(r"\item ").count(),
        2,
        "{}",
        custom_enum.output
    );
    assert!(
        custom_enum.output.contains(r"One \label{x}"),
        "{}",
        custom_enum.output
    );

    let or3 = typst_to_latex_with_diagnostics(
        "#show selector(<a>).or(<b>, <c>): [OR]\n= A <a>\n= B <b>\n= C <c>\n= D <d>",
        &T2LOptions::default(),
    );
    assert_eq!(or3.output.matches("OR").count(), 3, "{}", or3.output);
    assert!(or3.output.contains(r"\section{"), "{}", or3.output);
    assert!(or3.output.contains(r"\label{d}"), "{}", or3.output);

    let and3 = typst_to_latex_with_diagnostics(
        "#show selector(heading).and(heading.where(level: 1), <a1>): [AND]\n= One <a1>\n== Two <a1>",
        &T2LOptions::default(),
    );
    assert_eq!(and3.output.matches("AND").count(), 1, "{}", and3.output);
    assert!(and3.output.contains(r"\subsection{"), "{}", and3.output);

    let func_h = typst_to_latex_with_diagnostics(
        "#show heading: it => if it.func() == heading [MATCH-H] else [OTHER]\n= Title",
        &T2LOptions::default(),
    );
    assert_eq!(func_h.output.trim(), "MATCH-H");

    let func_li = typst_to_latex_with_diagnostics(
        "#show list.item: it => if it.func() == list.item [MATCH-LI] else [OTHER]\n- Item",
        &T2LOptions::default(),
    );
    assert_eq!(func_li.output.trim(), "MATCH-LI");
}

#[test]
fn show_replacement_with_new_headings_captionless_figures_and_explicit_kind() {
    let repl_new_heading = "#set heading(numbering: \"1.\")\n#show <x>: [= New <y>]\n= Orig <x>\n= After\nSee @x and @y.";
    let h_res = typst_to_latex_with_diagnostics(repl_new_heading, &T2LOptions::default());
    assert!(h_res.warnings.is_empty(), "warnings: {:?}", h_res.warnings);
    let anchor_pos = h_res
        .output
        .find(r"\refstepcounter{section}\label{x}")
        .expect(&h_res.output);
    let sec_new_pos = h_res.output.find(r"\section{ New}").expect(&h_res.output);
    let y_lbl_pos = h_res.output.find(r"\label{y}").expect(&h_res.output);
    assert!(
        anchor_pos < sec_new_pos && sec_new_pos < y_lbl_pos,
        "{}",
        h_res.output
    );
    assert!(
        h_res
            .output
            .contains(r"See Section~\ref{x} and Section~\ref{y}."),
        "{}",
        h_res.output
    );

    let closure_new_heading = "#set heading(numbering: \"1.\")\n#show <x>: it => [= New (#it.body) <y>]\n= Orig <x>\n= After\nSee @x and @y.";
    let hc_res = typst_to_latex_with_diagnostics(closure_new_heading, &T2LOptions::default());
    assert!(
        hc_res.output.contains(r"\refstepcounter{section}\label{x}"),
        "{}",
        hc_res.output
    );
    assert!(
        hc_res.output.contains(r"\section{ New (Orig)}"),
        "{}",
        hc_res.output
    );
    assert!(
        hc_res
            .output
            .contains(r"See Section~\ref{x} and Section~\ref{y}."),
        "{}",
        hc_res.output
    );

    let kept_plus_new = "#set heading(numbering: \"1.\")\n#show <x>: it => [#it\n= Extra <y>]\n= Orig <x>\n= After\nSee @x and @y.";
    let hk_res = typst_to_latex_with_diagnostics(kept_plus_new, &T2LOptions::default());
    assert!(
        !hk_res.output.contains(r"\refstepcounter"),
        "{}",
        hk_res.output
    );
    assert_eq!(
        hk_res.output.matches(r"\label{x}").count(),
        1,
        "{}",
        hk_res.output
    );
    assert_eq!(
        hk_res.output.matches(r"\label{y}").count(),
        1,
        "{}",
        hk_res.output
    );
    let sec_orig = hk_res
        .output
        .find(r"\section{ Orig}")
        .expect(&hk_res.output);
    let x_pos = hk_res.output.find(r"\label{x}").expect(&hk_res.output);
    let sec_extra = hk_res
        .output
        .find(r"\section{ Extra}")
        .expect(&hk_res.output);
    let y_pos = hk_res.output.find(r"\label{y}").expect(&hk_res.output);
    assert!(
        sec_orig < x_pos && x_pos < sec_extra && sec_extra < y_pos,
        "{}",
        hk_res.output
    );

    let fig_to_table = "#show <f1>: [#figure(table(columns: 2)[A][B], caption: [Tab]) <t1>]\n#figure([Img1], caption: [Cap1]) <f1>\n#figure([Img2], caption: [Cap2]) <f2>\nSee @f1, @t1, and @f2.";
    let ft_res = typst_to_latex_with_diagnostics(fig_to_table, &T2LOptions::default());
    let f1_anchor = ft_res
        .output
        .find(r"\refstepcounter{figure}\label{f1}")
        .expect(&ft_res.output);
    let tab_begin = ft_res.output.find(r"\begin{table}").expect(&ft_res.output);
    let t1_label = ft_res.output.find(r"\label{t1}").expect(&ft_res.output);
    let tab_end = ft_res.output.find(r"\end{table}").expect(&ft_res.output);
    let fig2_begin = ft_res.output.find(r"\begin{figure}").expect(&ft_res.output);
    let f2_label = ft_res.output.find(r"\label{f2}").expect(&ft_res.output);
    let fig2_end = ft_res.output.find(r"\end{figure}").expect(&ft_res.output);
    assert!(f1_anchor < tab_begin, "{}", ft_res.output);
    assert!(
        tab_begin < t1_label && t1_label < tab_end,
        "{}",
        ft_res.output
    );
    assert!(
        fig2_begin < f2_label && f2_label < fig2_end,
        "{}",
        ft_res.output
    );
    assert!(
        ft_res
            .output
            .contains(r"See Figure~\ref{f1}, Table~\ref{t1}, and Figure~\ref{f2}."),
        "{}",
        ft_res.output
    );

    let fig_to_captionless = "#show <f1>: [#figure([Other]) <f_inner>]\n#figure([Img1], caption: [Cap1]) <f1>\n#figure([Img2], caption: [Cap2]) <f2>\nSee @f1, @f_inner, and @f2.";
    let fc_res = typst_to_latex_with_diagnostics(fig_to_captionless, &T2LOptions::default());
    assert!(
        fc_res.output.contains(r"\refstepcounter{figure}\label{f1}"),
        "{}",
        fc_res.output
    );
    let fc_begin = fc_res.output.find(r"\begin{figure}").expect(&fc_res.output);
    let fc_step = fc_res.output[fc_begin..]
        .find(r"\refstepcounter{figure}")
        .map(|off| fc_begin + off)
        .expect(&fc_res.output);
    let fc_lbl = fc_res
        .output
        .find(r"\label{f_inner}")
        .expect(&fc_res.output);
    let fc_end = fc_res.output.find(r"\end{figure}").expect(&fc_res.output);
    assert!(
        fc_begin < fc_step && fc_step < fc_lbl && fc_lbl < fc_end,
        "{}",
        fc_res.output
    );
    assert!(
        fc_res
            .output
            .contains(r"See Figure~\ref{f1}, Figure~\ref{f_inner}, and Figure~\ref{f2}."),
        "{}",
        fc_res.output
    );

    let keep_captionless =
        "#show <f1>: it => [Wrap: #it]\n#figure([Img1]) <f1>\n#figure([Img2], caption: [Cap2]) <f2>\nSee @f1 and @f2.";
    let kc_res = typst_to_latex_with_diagnostics(keep_captionless, &T2LOptions::default());
    assert_eq!(
        kc_res.output.matches(r"\refstepcounter{figure}").count(),
        1,
        "{}",
        kc_res.output
    );
    let kc_begin = kc_res.output.find(r"\begin{figure}").expect(&kc_res.output);
    let kc_step = kc_res
        .output
        .find(r"\refstepcounter{figure}")
        .expect(&kc_res.output);
    let kc_lbl = kc_res.output.find(r"\label{f1}").expect(&kc_res.output);
    let kc_end = kc_res.output.find(r"\end{figure}").expect(&kc_res.output);
    assert!(
        kc_begin < kc_step && kc_step < kc_lbl && kc_lbl < kc_end,
        "{}",
        kc_res.output
    );
    assert!(
        kc_res
            .output
            .contains(r"See Figure~\ref{f1} and Figure~\ref{f2}."),
        "{}",
        kc_res.output
    );

    let explicit_image_replaced = "#show <f1>: [REPL]\n#figure(table(columns: 2)[A][B], kind: image, caption: [Cap]) <f1>\nSee @f1.";
    let ei_res = typst_to_latex_with_diagnostics(explicit_image_replaced, &T2LOptions::default());
    assert!(
        ei_res
            .output
            .contains(r"\refstepcounter{figure}\label{f1}REPL"),
        "{}",
        ei_res.output
    );
    assert!(
        ei_res.output.contains(r"Figure~\ref{f1}"),
        "{}",
        ei_res.output
    );

    let explicit_image_unreplaced =
        "#figure(table(columns: 2)[A][B], kind: image, caption: [Cap]) <f1>\nSee @f1.";
    for out in [
        typst_to_latex_with_diagnostics(explicit_image_unreplaced, &T2LOptions::default()).output,
        typst_to_latex(explicit_image_unreplaced),
    ] {
        assert!(out.contains(r"\begin{figure}"), "{out}");
        assert!(out.contains(r"Figure~\ref{f1}"), "{out}");
        assert!(!out.contains(r"\begin{table}"), "{out}");
    }

    let explicit_table = "#figure([Text], kind: table, caption: [Cap]) <t1>\nSee @t1.";
    for out in [
        typst_to_latex_with_diagnostics(explicit_table, &T2LOptions::default()).output,
        typst_to_latex(explicit_table),
    ] {
        assert!(out.contains(r"\begin{table}"), "{out}");
        assert!(out.contains(r"Table~\ref{t1}"), "{out}");
    }
}

#[test]
fn repeated_text_duplicate_show_and_filtered_set_rules() {
    let long_src = format!("#show \"a\": [b]\n{}", "a".repeat(600));
    let long_res = typst_to_latex_with_diagnostics(&long_src, &T2LOptions::default());
    assert_eq!(long_res.output.trim(), "b".repeat(600));

    let dup_h_lbl = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.\")\n#show <x>: it => [\n#it\n#it\n]\n= First <x>\n= Second\nSee @x.",
        &T2LOptions::default(),
    );
    assert_eq!(
        dup_h_lbl.output.matches(r"\label{x}").count(),
        1,
        "{}",
        dup_h_lbl.output
    );
    assert!(
        dup_h_lbl.output.contains(r"\addtocounter{section}{-1}"),
        "{}",
        dup_h_lbl.output
    );
    assert!(
        dup_h_lbl.output.contains(r"Section~\ref{x}"),
        "{}",
        dup_h_lbl.output
    );

    let dup_h_elem = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.\")\n#show heading: it => [\n#it\n#it\n]\n= First <x>\n= Second\nSee @x.",
        &T2LOptions::default(),
    );
    assert_eq!(
        dup_h_elem.output.matches(r"\label{x}").count(),
        1,
        "{}",
        dup_h_elem.output
    );
    assert_eq!(
        dup_h_elem
            .output
            .matches(r"\addtocounter{section}{-1}")
            .count(),
        2,
        "{}",
        dup_h_elem.output
    );

    let tight_list = typst_to_latex_with_diagnostics(
        "#show list.where(tight: true): set list(marker: [Q])\n- tight1\n- tight2\n\nPara\n\n- loose1\n\n- loose2",
        &T2LOptions::default(),
    );
    assert!(
        tight_list.output.contains(r"\item[Q] tight1"),
        "{}",
        tight_list.output
    );
    assert!(
        tight_list.output.contains(r"\item loose1"),
        "{}",
        tight_list.output
    );

    let heading_where = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.\")\n#show heading.where(level: 2): set heading(numbering: none)\n= One\n== SubNoNum\n= Two",
        &T2LOptions::default(),
    );
    assert!(
        heading_where.output.contains(r"\subsection*{SubNoNum}"),
        "{}",
        heading_where.output
    );
    assert!(
        heading_where.output.contains(r"\section{ One}")
            && heading_where.output.contains(r"\section{ Two}"),
        "{}",
        heading_where.output
    );

    let enum_mixed = "#set enum(numbering: \"I.\")\n3. Three\n4. Four\n9. Nine\n10. Ten";
    for out in [
        typst_to_latex_with_diagnostics(enum_mixed, &T2LOptions::default()).output,
        typst_to_latex(enum_mixed),
    ] {
        assert!(
            out.contains(r"\renewcommand{\labelenumi}{\Roman{enumi}.}"),
            "{out}"
        );
        assert!(out.contains(r"\setcounter{enumi}{2}"), "{out}");
        assert!(out.contains(r"\setcounter{enumi}{8}"), "{out}");
    }

    let utf8_lens = typst_to_latex_with_diagnostics(
        "#(\"é\".len()) / #(\"😀\".len()) / #(len(\"é\")) / #(len(\"😀\"))",
        &T2LOptions::default(),
    );
    assert_eq!(utf8_lens.output.trim(), "2 / 4 / 2 / 4");
}

#[test]
fn figure_caption_custom_kind_and_paren_table_consistency() {
    let num_none = "#figure([Img1], caption: [UnnumCap], numbering: none)\n#figure([Img2], caption: [Cap2]) <f2>\nSee @f2.";
    for out in [
        typst_to_latex_with_diagnostics(num_none, &T2LOptions::default()).output,
        typst_to_latex(num_none),
    ] {
        assert!(!out.contains(r"\caption{UnnumCap}"), "{out}");
        assert!(out.contains("UnnumCap"), "{out}");
        assert!(out.contains(r"\caption{Cap2}"), "{out}");
        assert!(out.contains(r"Figure~\ref{f2}"), "{out}");
    }

    let cap_none = "#figure([Img1], caption: none) <f1>\n#figure([Img2], caption: [Cap2]) <f2>\nSee @f1 and @f2.";
    for out in [
        typst_to_latex_with_diagnostics(cap_none, &T2LOptions::default()).output,
        typst_to_latex(cap_none),
    ] {
        assert!(!out.contains(r"\caption{}"), "{out}");
        assert!(out.contains(r"\refstepcounter{figure}"), "{out}");
        assert!(out.contains(r"Figure~\ref{f1}"), "{out}");
    }

    let custom_diagram = "#figure([Diag1], kind: \"diagram\", supplement: [Diagram], caption: [D1]) <d1>\n#figure([Img1], caption: [Cap1]) <f1>\nSee @d1 and @f1.";
    for out in [
        typst_to_latex_with_diagnostics(custom_diagram, &T2LOptions::default()).output,
        typst_to_latex(custom_diagram),
    ] {
        assert!(out.contains(r"\newcounter{tylaxfigdiagram}"), "{out}");
        assert!(out.contains(r"\refstepcounter{tylaxfigdiagram}"), "{out}");
        assert!(!out.contains(r"\caption{D1}"), "{out}");
        assert!(out.contains(r"Diagram~\thetylaxfigdiagram: D1"), "{out}");
        assert!(out.contains(r"\label{d1}"), "{out}");
        assert!(out.contains(r"\caption{Cap1}"), "{out}");
        assert!(
            out.contains(r"Diagram~\ref{d1} and Figure~\ref{f1}"),
            "{out}"
        );
    }

    let paren_nested = "#figure([Img], kind: (table), caption: [ParenKind]) <t1>\n#figure((table(columns: 2)[A][B]), caption: [ParenBody]) <t2>\n#figure(align(center, table(columns: 2)[C][D]), caption: [AlignTable]) <t3>\n#figure([#align(center)[#table(columns: 2)[E][F]]], caption: [BlockAlignTable]) <t4>\nSee @t1, @t2, @t3, @t4.";
    for out in [
        typst_to_latex_with_diagnostics(paren_nested, &T2LOptions::default()).output,
        typst_to_latex(paren_nested),
    ] {
        assert_eq!(out.matches(r"\begin{table}").count(), 4, "{out}");
        assert!(!out.contains(r"\begin{figure}"), "{out}");
        assert!(out.contains("A & B"), "{out}");
        assert!(out.contains("C & D"), "{out}");
        assert!(out.contains("E & F"), "{out}");
        for lbl in ["t1", "t2", "t3", "t4"] {
            assert!(out.contains(&format!(r"Table~\ref{{{lbl}}}")), "{out}");
        }
    }
}

#[test]
fn include_preserves_adjacent_list_grouping_with_parent_show() {
    run_without_typst_root(
        "include_preserves_adjacent_list_grouping_with_parent_show",
        || {
            let dir =
                std::env::temp_dir().join(format!("tylax_pr69_inc_list_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            std::fs::write(dir.join("part_plain.typ"), "- one\n").unwrap();
            let merged = dir.join("main_merged.typ");
            let merged_src = "#show list: [GROUP]\n#include \"part_plain.typ\"\n- two";
            std::fs::write(&merged, merged_src).unwrap();
            let merged_res =
                typst_file_to_latex_with_diagnostics(merged_src, &merged, &T2LOptions::default());
            assert_eq!(merged_res.output.trim(), "GROUP", "{}", merged_res.output);

            std::fs::write(dir.join("part_show.typ"), "#show list: [CHILD]\n- one\n").unwrap();
            let split = dir.join("main_split.typ");
            let split_src = "#show list: [OUTER]\n#include \"part_show.typ\"\n- two";
            std::fs::write(&split, split_src).unwrap();
            let split_res =
                typst_file_to_latex_with_diagnostics(split_src, &split, &T2LOptions::default());
            assert_eq!(
                split_res.output.trim(),
                "CHILD OUTER",
                "{}",
                split_res.output
            );

            let _ = std::fs::remove_dir_all(&dir);
        },
    );
}

#[test]
fn interleaved_duplicates_explicit_overrides_set_figure_and_case_kinds() {
    // 1. Interleaved duplicate headings preserve 1, 2, 1, 3 and subsection numbering (1.1, 1.2 / 1.1, 2.1)
    let inter_h1 = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.1\")\n#show <x>: it => [\n#it\n= Middle <m>\n#it\n]\n= First <x>\n= After <a>\nRefs: @x, @m, @a.",
        &T2LOptions::default(),
    );
    assert!(
        inter_h1.output.contains("tylaxorig:"),
        "{}",
        inter_h1.output
    );
    assert!(
        inter_h1.output.contains(r"\tylaxsavedsection"),
        "{}",
        inter_h1.output
    );
    assert_eq!(
        inter_h1.output.matches(r"\label{x}").count(),
        1,
        "{}",
        inter_h1.output
    );

    let inter_sub = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.1\")\n#show <x>: it => [\n#it\n== Sub1 <s1>\n#it\n== Sub2 <s2>\n]\n= First <x>",
        &T2LOptions::default(),
    );
    assert!(
        inter_sub
            .output
            .contains(r"\setcounter{subsection}{\tylaxsavedsubsection}"),
        "{}",
        inter_sub.output
    );

    // 2. Explicit element arguments survive `show ...: set ...`
    let explicit_list = typst_to_latex_with_diagnostics(
        "#show list.where(tight: true): set list(marker: [Q])\n#list(marker: [X], [one], [two])\n- a\n- b",
        &T2LOptions::default(),
    );
    assert!(
        explicit_list.output.contains(r"\item[X] one"),
        "{}",
        explicit_list.output
    );
    assert!(
        explicit_list.output.contains(r"\item[Q] a"),
        "{}",
        explicit_list.output
    );
    assert!(
        !explicit_list.output.contains("__tylax_"),
        "{}",
        explicit_list.output
    );

    let explicit_heading = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.\")\n#show heading.where(level: 2): set heading(numbering: none)\n= One\n#heading(level: 2, numbering: \"A.\")[ExplicitNum] <h2e>\n== ImplicitSub\nSee @h2e.",
        &T2LOptions::default(),
    );
    assert!(
        explicit_heading
            .output
            .contains(r"\subsection{ExplicitNum}"),
        "{}",
        explicit_heading.output
    );
    assert!(
        explicit_heading
            .output
            .contains(r"\subsection*{ImplicitSub}"),
        "{}",
        explicit_heading.output
    );
    assert!(
        explicit_heading.output.contains(r"Section~\ref{h2e}"),
        "{}",
        explicit_heading.output
    );

    let explicit_enum = typst_to_latex_with_diagnostics(
        "#show enum.where(tight: true): set enum(start: 5, numbering: \"I.\")\n#enum(start: 2, numbering: \"a.\", [one], [two])\n+ c\n+ d",
        &T2LOptions::default(),
    );
    assert!(
        explicit_enum
            .output
            .contains(r"\renewcommand{\labelenumi}{\alph{enumi}.}")
            && explicit_enum.output.contains(r"\setcounter{enumi}{1}"),
        "{}",
        explicit_enum.output
    );
    assert!(
        explicit_enum
            .output
            .contains(r"\renewcommand{\labelenumi}{\Roman{enumi}.}")
            && explicit_enum.output.contains(r"\setcounter{enumi}{4}"),
        "{}",
        explicit_enum.output
    );

    // 3a. `#set figure(numbering: none)` in both eval and `--no-eval`
    let set_fig = "#set figure(numbering: none)\n#figure([Img1], caption: [UnnumCap])\n#set figure(numbering: \"1\")\n#figure([Img2], caption: [NumCap]) <f2>\nSee @f2.";
    for out in [
        typst_to_latex_with_diagnostics(set_fig, &T2LOptions::default()).output,
        typst_to_latex(set_fig),
    ] {
        assert!(!out.contains(r"\caption{UnnumCap}"), "{out}");
        assert!(out.contains("UnnumCap"), "{out}");
        assert!(out.contains(r"\caption{NumCap}"), "{out}");
        assert!(out.contains(r"Figure~\ref{f2}"), "{out}");
    }

    // 3b. Case-sensitive custom figure `kind` counters (`"Example"` vs `"example"`)
    let case_fig = "#figure([E1], kind: \"Example\", supplement: [Example], caption: [Upper1]) <e1>\n#figure([e2], kind: \"example\", supplement: [example], caption: [Lower1]) <e2>\n#figure([E3], kind: \"Example\", supplement: [Example], caption: [Upper2]) <e3>\nSee @e1, @e2, and @e3.";
    for out in [
        typst_to_latex_with_diagnostics(case_fig, &T2LOptions::default()).output,
        typst_to_latex(case_fig),
    ] {
        assert!(out.contains(r"\newcounter{tylaxfigExample}"), "{out}");
        assert!(out.contains(r"\newcounter{tylaxfigexample}"), "{out}");
        assert!(
            out.contains(r"Example~\ref{e1}, example~\ref{e2}, and Example~\ref{e3}"),
            "{out}"
        );
    }

    // 4. Interaction between duplicating `#show` rules and `show ...: set ...` / chained closures
    let same_set_dup = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.1\")\n#show heading.where(level: 1): set heading(numbering: \"1.1\")\n#show <x>: it => [\n#it\n= Mid <m>\n#it\n]\n= First <x>\nSee @x and @m.",
        &T2LOptions::default(),
    );
    assert_eq!(
        same_set_dup.output.matches(r"\label{x}").count(),
        1,
        "{}",
        same_set_dup.output
    );
    assert_eq!(
        same_set_dup.output.matches(r"\label{m}").count(),
        1,
        "{}",
        same_set_dup.output
    );
    assert!(
        same_set_dup.output.contains("tylaxorig:")
            && same_set_dup.output.contains(r"Section~\ref{x}")
            && same_set_dup.output.contains(r"Section~\ref{m}"),
        "{}",
        same_set_dup.output
    );

    let show_set_only_dup = typst_to_latex_with_diagnostics(
        "#show heading.where(level: 1): set heading(numbering: \"1.1\")\n#show <x>: it => [\n#it\n= Mid <m>\n#it\n]\n= First <x>\nSee @x and @m.",
        &T2LOptions::default(),
    );
    assert_eq!(
        show_set_only_dup.output.matches(r"\section{ Mid}").count(),
        1,
        "{}",
        show_set_only_dup.output
    );
    assert_eq!(
        show_set_only_dup.output.matches(r"\label{x}").count(),
        1,
        "{}",
        show_set_only_dup.output
    );
    assert!(
        show_set_only_dup.output.contains("tylaxorig:")
            && show_set_only_dup.output.contains(r"Section~\ref{x}")
            && show_set_only_dup.output.contains(r"Section~\ref{m}"),
        "{}",
        show_set_only_dup.output
    );

    let two_closure_dup = typst_to_latex_with_diagnostics(
        "#set heading(numbering: \"1.1\")\n#show heading.where(level: 1): it => [W #it]\n#show <x>: it => [\n#it\n= Mid <m>\n#it\n]\n= First <x>\nSee @x and @m.",
        &T2LOptions::default(),
    );
    assert_eq!(
        two_closure_dup.output.matches(r"\label{x}").count(),
        1,
        "{}",
        two_closure_dup.output
    );
    assert_eq!(
        two_closure_dup.output.matches(r"\label{m}").count(),
        1,
        "{}",
        two_closure_dup.output
    );
    assert_eq!(
        two_closure_dup.output.matches("W ").count(),
        3,
        "{}",
        two_closure_dup.output
    );
    assert!(
        two_closure_dup.output.contains("tylaxorig:")
            && two_closure_dup.output.contains(r"Section~\ref{x}")
            && two_closure_dup.output.contains(r"Section~\ref{m}"),
        "{}",
        two_closure_dup.output
    );
}
