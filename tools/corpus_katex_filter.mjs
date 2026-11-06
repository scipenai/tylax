#!/usr/bin/env node
// Keep only the formulas that KaTeX accepts, so that the compile audit sees
// valid LaTeX math and not OCR or extraction noise.
//
// Usage:
//     node tools/corpus_katex_filter.mjs <in.json|in.txt> <out.json>
//
// The input is a JSON array of {id, latex} or plain text with one formula per
// line; the output is a JSON array of {id, latex}. KaTeX is loaded from
// ~/.cache/tylax/node (install it there with
// `npm install --prefix ~/.cache/tylax/node katex`), not from the repository.

import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { basename, extname, join } from "node:path";

const require = createRequire(join(homedir(), ".cache", "tylax", "node", "package.json"));
const katex = require("katex");
// KaTeX warns about each character it has no metrics for; the audit does not care.
console.warn = () => {};

const [input, output] = process.argv.slice(2);
if (!input || !output) {
  console.error("usage: node tools/corpus_katex_filter.mjs <in.json|in.txt> <out.json>");
  process.exit(2);
}

const text = readFileSync(input, "utf8");
const stem = basename(input, extname(input));
const cases = text.trimStart().startsWith("[")
  ? JSON.parse(text)
  : text
      .split("\n")
      .map((latex, index) => ({ id: `${stem}:${index + 1}`, latex: latex.replace(/\r$/, "") }))
      .filter((c) => c.latex.trim() !== "");

const kept = [];
const reasons = new Map();
for (const c of cases) {
  try {
    katex.renderToString(c.latex, { throwOnError: true, displayMode: true, strict: "ignore" });
    kept.push({ id: c.id, latex: c.latex });
  } catch (error) {
    // "KaTeX parse error: Undefined control sequence: \foo at position 3: …"
    const reason = String(error.message)
      .replace(/^KaTeX parse error: /, "")
      .replace(/ at (?:position|end of input).*$/s, "")
      .replace(/\\[A-Za-z]+/g, (name) => (name.length > 1 ? "\\cmd" : name));
    reasons.set(reason, (reasons.get(reason) ?? 0) + 1);
  }
}
writeFileSync(output, "[" + kept.map((c) => JSON.stringify(c)).join(",\n") + "]\n");

console.log(`${input}: ${cases.length} formulas, ${kept.length} accepted by KaTeX -> ${output}`);
const top = [...reasons.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
for (const [reason, count] of top) console.log(`  ${String(count).padStart(6)}  ${reason}`);
