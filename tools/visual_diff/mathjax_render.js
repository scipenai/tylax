// Render LaTeX math to standalone SVG with MathJax, for the visual diff.
//
// Usage: node mathjax_render.js <in.jsonl> <out-dir> <px-per-em>
//
// Reads JSONL records with a "latex" field and writes <out-dir>/<n>.svg for
// record n (zero-based, five digits), in display style. The width and height
// are set in pixels at <px-per-em>, so the SVG rasterises at the same scale as
// Typst at the matching --ppi. Prints one JSON line per record to stdout:
// {"index", "ok", "error"}, where "ok" is false when MathJax reports a TeX
// error (an unknown macro, a missing argument, ...).
//
// MathJax is loaded from $TYLAX_NODE_DIR, by default ~/.cache/tylax/node,
// outside the repository (see visual_diff.py for the install command).

const fs = require('fs');
const os = require('os');
const path = require('path');
const readline = require('readline');
const { createRequire } = require('module');

const nodeDir = process.env.TYLAX_NODE_DIR || path.join(os.homedir(), '.cache/tylax/node');
const load = createRequire(path.join(nodeDir, 'package.json'));
const { mathjax } = load('mathjax-full/js/mathjax.js');
const { TeX } = load('mathjax-full/js/input/tex.js');
const { SVG } = load('mathjax-full/js/output/svg.js');
const { liteAdaptor } = load('mathjax-full/js/adaptors/liteAdaptor.js');
const { RegisterHTMLHandler } = load('mathjax-full/js/handlers/html.js');
const { AllPackages } = load('mathjax-full/js/input/tex/AllPackages.js');

const [inPath, outDir, pxPerEmArg] = process.argv.slice(2);
const pxPerEm = Number(pxPerEmArg || 40);

const adaptor = liteAdaptor();
RegisterHTMLHandler(adaptor);
// `noundefined` and `noerrors` would hide exactly the failures to skip.
const packages = AllPackages.filter((p) => p !== 'noundefined' && p !== 'noerrors');
const tex = new TeX({ packages });
// fontCache 'none' inlines every glyph path, so the SVG needs no <defs> lookup.
const svg = new SVG({ fontCache: 'none' });
const doc = mathjax.document('', { InputJax: tex, OutputJax: svg });

function render(latex) {
  const node = doc.convert(latex, { display: true });
  let markup = adaptor.innerHTML(node);
  const error = /data-mjx-error="([^"]*)"/.exec(markup);
  if (error) return { error: error[1] || 'TeX error' };
  const box = /viewBox="([-\d.]+) ([-\d.]+) ([-\d.]+) ([-\d.]+)"/.exec(markup);
  if (!box) return { error: 'no viewBox' };
  // The viewBox is in thousandths of an em.
  const width = (Number(box[3]) / 1000) * pxPerEm;
  const height = (Number(box[4]) / 1000) * pxPerEm;
  markup = markup
    .replace(/ width="[^"]*"/, ` width="${width.toFixed(2)}px"`)
    .replace(/ height="[^"]*"/, ` height="${height.toFixed(2)}px"`)
    .replace(/ style="[^"]*"/, '')
    .replace(/currentColor/g, 'black');
  return { markup };
}

async function main() {
  fs.mkdirSync(outDir, { recursive: true });
  const lines = readline.createInterface({ input: fs.createReadStream(inPath) });
  let index = 0;
  for await (const line of lines) {
    if (!line.trim()) continue;
    const { latex } = JSON.parse(line);
    let result;
    try {
      result = render(latex);
    } catch (e) {
      result = { error: String(e && e.message ? e.message : e) };
    }
    if (result.markup) {
      const name = path.join(outDir, `${String(index).padStart(5, '0')}.svg`);
      fs.writeFileSync(name, result.markup);
    }
    process.stdout.write(JSON.stringify({ index, ok: !result.error, error: result.error || null }) + '\n');
    index += 1;
  }
}

main();
