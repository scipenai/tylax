//! Parse-check target for `tylax::latex_to_typst`.
//!
//! For input that is plausibly valid LaTeX math, the Typst it emits must parse
//! with no error nodes once wrapped as an equation (`$ ... $`), which is how it
//! is used. `typst_syntax::parse_math` on the bare output is stricter than the
//! compiler: it rejects a lone `(`, which compiles fine inside `$ ... $`.
//!
//! Invalid input is skipped: broken LaTeX (an unclosed brace, a `$` inside
//! math) gives broken Typst, and those cases drown out the real bugs.

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Cheap syntactic checks that reject most invalid LaTeX math. It is not a
/// LaTeX parser: some invalid input gets through, which triage then sorts out.
fn plausibly_valid(input: &str) -> bool {
    if input
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return false;
    }
    // Math delimiters and comments have no meaning inside a math fragment.
    if input.contains(['$', '%', '#']) {
        return false;
    }
    let bytes = input.as_bytes();
    let mut depth = 0i32;
    let mut envs: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                let rest = &input[i + 1..];
                if rest.starts_with(['(', ')', '[', ']']) {
                    return false;
                }
                for (prefix, opens) in [("begin{", true), ("end{", false)] {
                    if let Some(after) = rest.strip_prefix(prefix) {
                        let Some(close) = after.find('}') else {
                            return false;
                        };
                        let name = &after[..close];
                        if name.is_empty()
                            || !name.chars().all(|c| c.is_ascii_alphabetic() || c == '*')
                        {
                            return false;
                        }
                        if opens {
                            envs.push(name);
                        } else if envs.pop() != Some(name) {
                            return false;
                        }
                    }
                }
                // Skip the escaped character so `\{` and `\}` do not count as braces.
                i += 2;
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            // A script needs an argument.
            b'^' | b'_' => {
                let next = input[i + 1..].trim_start();
                if next.is_empty()
                    || next.starts_with(['}', '^', '_', '&'])
                    || next.starts_with("\\\\")
                {
                    return false;
                }
            }
            _ => {}
        }
        i += 1;
    }
    depth == 0 && envs.is_empty()
}

/// Whether the output calls an unknown command as a function. tylax passes an
/// unknown `\foo` through as `foo`, so `\foo(x` becomes the unclosed call
/// `foo(x`. The input is invalid LaTeX (an undefined command), so skip it.
fn calls_unknown_command(input: &str, output: &str) -> bool {
    input.split('\\').skip(1).any(|rest| {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        !name.is_empty()
            && tylax::latex_to_typst(&format!("\\{name}")).trim() == name
            && output.contains(&format!("{name}("))
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    if !plausibly_valid(input) {
        return;
    }
    let output = tylax::latex_to_typst(input);
    let root = typst_syntax::parse(&format!("$ {output} $"));
    if root.erroneous() && !calls_unknown_command(input, &output) {
        let errors: Vec<_> = root.errors().into_iter().map(|e| e.message).collect();
        panic!("output does not parse as Typst math\ninput:  {input:?}\noutput: {output:?}\nerrors: {errors:?}");
    }
});
