/**
 * Regression tests for LaTeX structural detection.
 *
 * Run with: `node --test` (from web/) — uses only Node built-ins, no deps.
 *
 * The central guarantee: literal source regions (`%` comments, inline `\verb`,
 * verbatim-like environments) must never be mistaken for real document
 * structure. Regression for the web-detection bug where `\verb|\begin{document}|`
 * was routed to the document converter.
 */
import test from 'node:test';
import assert from 'node:assert/strict';

import {
    isFullLatexDocument,
    isLatexDocument,
    maskLatexLiteralRegions,
} from './latex-detect.js';

test('a real \\begin{document} is detected', () => {
    assert.equal(isFullLatexDocument('\\begin{document}Hello\\end{document}'), true);
});

test('\\documentclass is detected', () => {
    assert.equal(isFullLatexDocument('\\documentclass{article}'), true);
});

test('inline \\verb hiding \\begin{document} is NOT a document', () => {
    // The reported bug: this is a math/text fragment, not a full document.
    assert.equal(isFullLatexDocument('\\verb|\\begin{document}|'), false);
});

test('inline \\verb hiding \\documentclass is NOT a document', () => {
    assert.equal(isFullLatexDocument('\\verb!\\documentclass{article}!'), false);
});

test('\\verb* form is also treated as literal', () => {
    assert.equal(isFullLatexDocument('\\verb*+\\begin{document}+'), false);
});

test('verbatim environment body is not real structure', () => {
    const input = '\\begin{verbatim}\n\\begin{document}\n\\section{X}\n\\end{document}\n\\end{verbatim}';
    assert.equal(isFullLatexDocument(input), false);
});

test('lstlisting body is not real structure', () => {
    const input = '\\begin{lstlisting}\n\\documentclass{article}\n\\end{lstlisting}';
    assert.equal(isFullLatexDocument(input), false);
});

test('comment hiding \\begin{document} is not a document', () => {
    assert.equal(isFullLatexDocument('% \\begin{document}\nx^2'), false);
});

test('escaped \\%-comment does not swallow a following real command', () => {
    // The `\%` is a literal percent, so the real \documentclass still counts.
    assert.equal(isFullLatexDocument('50\\% off \\documentclass{article}'), true);
});

test('real document alongside a verbatim decoy is still detected', () => {
    const input = '\\documentclass{article}\n\\begin{verbatim}\\end{document}\\end{verbatim}\n\\begin{document}Hi\\end{document}';
    assert.equal(isFullLatexDocument(input), true);
});

test('\\beginning is not mistaken for \\begin', () => {
    assert.equal(isFullLatexDocument('\\beginning{document}'), false);
});

test('isLatexDocument ignores \\verb-wrapped usepackage', () => {
    assert.equal(isLatexDocument('\\verb|\\usepackage{amsmath}|'), false);
});

test('isLatexDocument still detects a real \\usepackage', () => {
    assert.equal(isLatexDocument('\\usepackage{amsmath}\nx'), true);
});

test('masking blanks verbatim content but preserves length and newlines', () => {
    const input = '\\verb|abc|\nX';
    const masked = maskLatexLiteralRegions(input);
    assert.equal(masked.length, input.length);
    assert.equal(masked.includes('abc'), false);
    // Content outside the literal region is untouched.
    assert.equal(masked.endsWith('\nX'), true);
});

test('comment-separated verbatim block hides its document markers', () => {
    // TeX allows a `%` comment between \begin/\end and {verbatim}. The literal
    // \begin{document} inside must not be seen as real structure.
    const input = [
        '\\begin% note',
        '{verbatim}',
        '\\begin{document}',
        '\\end{document}',
        '\\end% close',
        '{verbatim}',
    ].join('\n');
    assert.equal(isFullLatexDocument(input), false);
});

test('comment-separated real \\begin{document} is still detected', () => {
    // The positive counterpart: outside verbatim, a comment separator between
    // \begin and {document} must NOT hide a genuine document start.
    const input = '\\begin% here\n{document}\nHi\n\\end{document}';
    assert.equal(isFullLatexDocument(input), true);
});

test('whitespace-separated verbatim tag is handled', () => {
    const input = '\\begin {verbatim}\n\\documentclass{article}\n\\end {verbatim}';
    assert.equal(isFullLatexDocument(input), false);
});

test('a bare math fragment is never a document', () => {
    assert.equal(isFullLatexDocument('\\frac{1}{2} + \\alpha^2'), false);
});
