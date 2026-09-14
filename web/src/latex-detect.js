/**
 * LaTeX structural detection (dependency-free, DOM-free).
 *
 * These helpers decide whether an input string is a full LaTeX document so the
 * app can route it to the document converter rather than the math converter.
 *
 * The core rule: literal source regions — `%` line comments, inline
 * `\verb`/`\verb*` spans, and verbatim-family environment bodies — must never
 * be mistaken for real document structure. For example `\verb|\begin{document}|`
 * is a math/text fragment, not a document. `maskLatexLiteralRegions` performs a
 * single left-to-right, escape-aware scan (mirroring the Rust
 * `shield_verbatim_regions` lexer) that blanks those regions before any
 * structural token is matched.
 */

/**
 * Verbatim-family environments whose body is literal source. Mirrors the Rust
 * `TRUE_VERBATIM_ENVS` + `SKIP_SCAN_ENVS`: their bodies must not be interpreted
 * as LaTeX for the purpose of document detection.
 */
const LATEX_VERBATIM_ENVS = [
    'verbatim',
    'verbatim*',
    'Verbatim',
    'Verbatim*',
    'lstlisting',
    'minted',
];

/**
 * True if the character at `index` is escaped by an odd run of backslashes.
 */
export function isEscapedLatexCharacter(input, index) {
    let precedingBackslashes = 0;
    for (let cursor = index - 1; cursor >= 0 && input[cursor] === '\\'; cursor -= 1) {
        precedingBackslashes += 1;
    }
    return precedingBackslashes % 2 === 1;
}

/**
 * Advance past the whitespace and `%` line comments that TeX allows between a
 * control word and its argument (e.g. `\begin% note\n{verbatim}`). Returns the
 * index of the first following significant character. Shared by the begin- and
 * end-tag scans so both honor the same separator rule as the Rust lexer.
 */
function skipSpacesAndComments(input, start) {
    let p = start;
    while (p < input.length) {
        const ch = input[p];
        if (/\s/.test(ch)) {
            p += 1;
        } else if (ch === '%' && !isEscapedLatexCharacter(input, p)) {
            const lineEnd = input.indexOf('\n', p);
            p = lineEnd === -1 ? input.length : lineEnd + 1;
        } else {
            break;
        }
    }
    return p;
}

/**
 * Replace `text` with a same-length string of spaces, preserving newlines so
 * line structure (and thus later `%`-comment handling) is unaffected.
 */
function blankOut(text) {
    let masked = '';
    for (const ch of text) {
        masked += ch === '\n' ? '\n' : ' ';
    }
    return masked;
}

/**
 * Parse a leading inline `\verb`/`\verb*` span at `input[i]` (which must be a
 * backslash). Returns the number of characters consumed, or `null` if this is
 * not a real `\verb`. Mirrors the Rust `parse_inline_verb`.
 */
function parseInlineVerb(input, i) {
    const token = '\\verb';
    if (!input.startsWith(token, i)) {
        return null;
    }
    let p = i + token.length;
    const first = input[p];
    if (first === undefined) {
        return null;
    }
    // A real inline `\verb` is followed by `*` or a non-letter delimiter, never
    // a letter (which would make it `\verbatim`, `\verbfoo`, …).
    if (/[A-Za-z]/.test(first)) {
        return null;
    }
    let delim;
    let delimIndex;
    if (first === '*') {
        delim = input[p + 1];
        delimIndex = p + 1;
        if (delim === undefined) {
            return null;
        }
    } else {
        delim = first;
        delimIndex = p;
    }
    const contentStart = delimIndex + 1;
    const close = input.indexOf(delim, contentStart);
    if (close === -1) {
        return null;
    }
    return { consumed: close + 1 - i };
}

/**
 * Parse a leading `\begin{ENV} … \end{ENV}` block for a verbatim-family
 * environment at `input[i]` (a backslash). Returns the number of characters
 * consumed, or `null` if this is not such a block. An unterminated block
 * consumes to end of input (its remainder is literal). Mirrors the Rust
 * `parse_env_block` gating on `TRUE_VERBATIM_ENVS`/`SKIP_SCAN_ENVS`.
 */
function parseVerbatimEnvBlock(input, i) {
    const beginTok = '\\begin';
    if (!input.startsWith(beginTok, i)) {
        return null;
    }
    // TeX allows whitespace and `%` comments before the environment argument.
    const argStart = skipSpacesAndComments(input, i + beginTok.length);
    if (input[argStart] !== '{') {
        return null;
    }
    const close = input.indexOf('}', argStart + 1);
    if (close === -1) {
        return null;
    }
    const env = input.slice(argStart + 1, close);
    if (!LATEX_VERBATIM_ENVS.includes(env)) {
        return null;
    }
    // Find the matching `\end{ENV}`, tolerating whitespace/comments after
    // `\end` (e.g. `\end% close\n{verbatim}`). A regex cannot skip `%`-comment
    // separators, so scan `\end` occurrences and validate each explicitly.
    const endTok = '\\end';
    const wanted = `{${env}}`;
    let scan = close + 1;
    while (true) {
        const endIdx = input.indexOf(endTok, scan);
        if (endIdx === -1) {
            // Unterminated: the remainder is literal, mirror the Rust lexer.
            return { consumed: input.length - i };
        }
        let q = endIdx + endTok.length;
        // `\end` must be a control word, not a prefix of `\endinput` etc.
        if (/[A-Za-z]/.test(input[q] ?? '')) {
            scan = q;
            continue;
        }
        q = skipSpacesAndComments(input, q);
        if (input.startsWith(wanted, q)) {
            return { consumed: q + wanted.length - i };
        }
        scan = endIdx + endTok.length;
    }
}

/**
 * Blank out every LaTeX literal region (comments, `\verb`, verbatim-like
 * environments) so structural detection cannot be fooled by literal text.
 * A single escape-aware left-to-right pass, matching the Rust lexer.
 */
export function maskLatexLiteralRegions(input) {
    let out = '';
    let i = 0;
    const n = input.length;

    while (i < n) {
        const ch = input[i];

        // `%` line comment. An escaped `\%` never reaches here because the
        // backslash branch below consumes the backslash together with its char.
        if (ch === '%') {
            const lineEnd = input.indexOf('\n', i);
            const end = lineEnd === -1 ? n : lineEnd;
            out += blankOut(input.slice(i, end));
            i = end;
            continue;
        }

        if (ch === '\\') {
            const verb = parseInlineVerb(input, i);
            if (verb) {
                out += blankOut(input.slice(i, i + verb.consumed));
                i += verb.consumed;
                continue;
            }
            const env = parseVerbatimEnvBlock(input, i);
            if (env) {
                out += blankOut(input.slice(i, i + env.consumed));
                i += env.consumed;
                continue;
            }
            // Any other command / escaped char: copy the backslash and the next
            // character as a unit so a following `%` or `{` is not reinterpreted.
            out += '\\';
            i += 1;
            if (i < n) {
                out += input[i];
                i += 1;
            }
            continue;
        }

        out += ch;
        i += 1;
    }

    return out;
}

/**
 * Scan `input` for a real `\begin{document}` — one that is not inside a comment
 * (callers should pass masked input, so verbatim/`\verb` regions are already
 * blanked). Escape- and prefix-aware.
 */
export function hasRealLatexBeginDocument(input) {
    let index = 0;

    while (index < input.length) {
        if (input[index] === '%' && !isEscapedLatexCharacter(input, index)) {
            const lineEnd = input.indexOf('\n', index);
            index = lineEnd === -1 ? input.length : lineEnd + 1;
            continue;
        }

        if (input.startsWith('\\begin', index) && !isEscapedLatexCharacter(input, index)) {
            const afterCommand = index + '\\begin'.length;
            // A control word cannot be a prefix of a longer command such as
            // `\beginning`. TeX otherwise ignores spaces and comments before
            // the environment argument.
            if (!/[A-Za-z]/.test(input[afterCommand] ?? '')) {
                let argumentStart = afterCommand;
                while (argumentStart < input.length) {
                    if (/\s/.test(input[argumentStart])) {
                        argumentStart += 1;
                    } else if (
                        input[argumentStart] === '%'
                        && !isEscapedLatexCharacter(input, argumentStart)
                    ) {
                        const lineEnd = input.indexOf('\n', argumentStart);
                        argumentStart = lineEnd === -1 ? input.length : lineEnd + 1;
                    } else {
                        break;
                    }
                }

                if (input.startsWith('{document}', argumentStart)) {
                    return true;
                }
            }
        }

        index += 1;
    }

    return false;
}

/**
 * Scan `input` for a real control word `\<command>` (callers should pass masked
 * input). Escape- and prefix-aware so `\sectioning` does not match `\section`.
 */
export function hasRealLatexControlWord(input, command) {
    const token = `\\${command}`;
    let index = 0;

    while (index < input.length) {
        if (input[index] === '%' && !isEscapedLatexCharacter(input, index)) {
            const lineEnd = input.indexOf('\n', index);
            index = lineEnd === -1 ? input.length : lineEnd + 1;
            continue;
        }

        if (
            input.startsWith(token, index)
            && !isEscapedLatexCharacter(input, index)
            && !/[A-Za-z]/.test(input[index + token.length] ?? '')
        ) {
            return true;
        }

        index += 1;
    }

    return false;
}

/**
 * True if the input looks like a full LaTeX document (has `\documentclass`,
 * `\begin{document}`, or a sectioning command paired with `\end`). Literal
 * regions are masked first, so document-looking text inside `\verb`/verbatim is
 * ignored.
 */
export function isFullLatexDocument(input) {
    const masked = maskLatexLiteralRegions(input);
    return hasRealLatexControlWord(masked, 'documentclass')
        || hasRealLatexBeginDocument(masked)
        || (hasRealLatexControlWord(masked, 'section') && hasRealLatexControlWord(masked, 'end'))
        || (hasRealLatexControlWord(masked, 'chapter') && hasRealLatexControlWord(masked, 'end'));
}

/**
 * Broader "is this a LaTeX document?" check used for preview/mode hints. Also
 * masks literal regions before matching structural commands.
 */
export function isLatexDocument(input) {
    const masked = maskLatexLiteralRegions(input);
    return isFullLatexDocument(masked)
        || hasRealLatexControlWord(masked, 'usepackage')
        || hasRealLatexControlWord(masked, 'section')
        || hasRealLatexControlWord(masked, 'chapter')
        || hasRealLatexControlWord(masked, 'title')
        || hasRealLatexControlWord(masked, 'maketitle');
}
