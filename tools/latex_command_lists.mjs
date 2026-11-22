#!/usr/bin/env node
// Print the LaTeX math commands that KaTeX and MathJax support, as JSON:
// {"katex": [names], "mathjax": {name: [packages]}}, names without the
// backslash. Used by tools/unmapped_commands.py.
//
// Usage:
//     node tools/latex_command_lists.mjs > commands.json
//
// KaTeX and MathJax are loaded from ~/.cache/tylax/node (install them with
// `npm install --prefix ~/.cache/tylax/node katex mathjax-full@3`).
// KaTeX does not export its tables, so its names are read from the
// defineSymbol, defineFunction and defineMacro calls in its ES module build.
// MathJax's names come from the symbol maps that its TeX packages register.

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { join } from "node:path";

const require = createRequire(join(homedir(), ".cache", "tylax", "node", "package.json"));
const NAME = /^[A-Za-z]+$/;

function katexNames() {
  const source = readFileSync(require.resolve("katex/dist/katex.mjs"), "utf8");
  const names = new Set();
  // Only math-mode symbols; text-mode ones (`\textdollar`) are not math.
  for (const m of source.matchAll(/defineSymbol\(math,[^;]*?"\\\\([A-Za-z]+)"/g)) names.add(m[1]);
  for (const m of source.matchAll(/names:\s*\[([^\]]*)\]/g)) {
    for (const n of m[1].matchAll(/["']\\\\([A-Za-z]+)["']/g)) names.add(n[1]);
  }
  for (const m of source.matchAll(/defineMacro\("\\\\([A-Za-z]+)"/g)) names.add(m[1]);
  return [...names].sort();
}

function mathjaxNames() {
  // The map registry is a closure variable, so record each map as it is
  // registered, before the packages load.
  const { MapHandler } = require("mathjax-full/js/input/tex/MapHandler.js");
  const maps = new Map();
  const register = MapHandler.register;
  MapHandler.register = (map) => {
    maps.set(map.name, map);
    register(map);
  };
  require("mathjax-full/js/input/tex/AllPackages.js");
  const { EnvironmentMap } = require("mathjax-full/js/input/tex/SymbolMap.js");
  const names = {};
  for (const [mapName, map] of maps) {
    // A RegExpMap (letters, digits) has no name table.
    if (map instanceof EnvironmentMap || !(map.map instanceof Map)) continue;
    for (const key of map.map.keys()) {
      const name = key.replace(/^\\/, "");
      if (!NAME.test(name)) continue;
      (names[name] ??= []).push(mapName);
    }
  }
  return names;
}

console.log(JSON.stringify({ katex: katexNames(), mathjax: mathjaxNames() }));
