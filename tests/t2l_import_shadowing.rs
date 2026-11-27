use tylax::{
    typst_file_to_latex_with_diagnostics, typst_file_to_latex_with_options, typst_to_latex,
    typst_to_latex_with_diagnostics, T2LOptions,
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
            assert_shadowed(
        "while_over_cap_widens_provider_to_unknown_shadowed.typ",
        "#let i = 0\n#let path = \"provider.typ\"\n#while i < 80 { if i == 70 { path = \"helper.typ\" }; i += 1 }\n#import path: *\n#terms.item([A], [B])",
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
