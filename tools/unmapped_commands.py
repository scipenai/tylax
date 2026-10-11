#!/usr/bin/env python3
"""List the LaTeX math commands that tylax does not convert to valid Typst.

Enumerates the commands of three sources:

- KaTeX's supported functions, symbols and macros (K);
- MathJax's TeX macros from all its packages (M);
- the unicode-math symbol table, unicode-math-table.tex (U).

KaTeX and MathJax names come from tools/latex_command_lists.mjs. Each command
is probed twice, as `\\name` and as `\\name{x}{y}`, through
tests/corpus_compile_audit.rs, which converts it with `latex_to_typst` and
compiles the output with `typst`. A command for which neither compiles is
unmapped or mapped wrongly; the output usually shows why (`\\check{x}{y}` ->
`check(x)y`, unknown variable). A command that compiles alone but not before a
group is mapped, but its output glues to what follows (`\\infty{x}` ->
`infinityx`). Commands that need particular arguments (`\\hspace{x}`) can
show up as unmapped although they are not.

A command can also compile by accident: the bare LaTeX name passes through and
happens to be a Typst name too (`\\gt` -> `gt`, which is right). Such commands, when the name
appears nowhere in src/data/ or src/core/latex2typst/ as a string literal, are
listed separately for a quick look.

Usage:
    python3 -I tools/unmapped_commands.py [--corpus FILE ...] [--top N]
        [--commands commands.json] [--unicode-math unicode-math-table.tex]

--corpus counts how often each command occurs in these corpora (JSON arrays of
{id, latex} or one formula per line), to sort the list by frequency.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COMMAND = re.compile(r"\\([A-Za-z]+)")
UNICODE_MATH = re.compile(r'\\UnicodeMathSymbol\{"([0-9A-Fa-f]+)\}\{\\([A-Za-z]+)\s*\}\{\\(\w+)\s*\}')
# Commands that only make sense with a partner or an environment name.
STRUCTURAL = {"begin", "end", "left", "right", "middle"}


def katex_mathjax(path: str | None) -> tuple[set, dict]:
    if path:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    else:
        script = ROOT / "tools" / "latex_command_lists.mjs"
        data = json.loads(subprocess.run(["node", str(script)], check=True, capture_output=True, text=True).stdout)
    return set(data["katex"]), data["mathjax"]


def unicode_math(path: str | None) -> dict:
    if not path:
        found = subprocess.run(["kpsewhich", "unicode-math-table.tex"], capture_output=True, text=True).stdout.strip()
        if not found:
            sys.exit("unicode-math-table.tex not found; pass --unicode-math")
        path = found
    table = {}
    for match in UNICODE_MATH.finditer(Path(path).read_text(encoding="utf-8")):
        table[match.group(2)] = (chr(int(match.group(1), 16)), match.group(3))
    return table


def corpus_counts(paths: list[str]) -> Counter:
    counts = Counter()
    for path in paths:
        text = Path(path).read_text(encoding="utf-8")
        if text.lstrip().startswith("["):
            formulas = [case["latex"] for case in json.loads(text)]
        else:
            formulas = text.split("\n")
        for latex in formulas:
            counts.update(COMMAND.findall(latex))
    return counts


def known_literals() -> set:
    """Every double-quoted identifier in the converter's tables and code."""
    literals = set()
    for directory in ("src/data", "src/core/latex2typst"):
        for path in (ROOT / directory).rglob("*.rs"):
            literals.update(re.findall(r'"\\*([A-Za-z]+)"', path.read_text(encoding="utf-8")))
    return literals


def probe(names: list[str]) -> tuple[dict, dict]:
    """Convert and compile `\\name` (id `name`) and `\\name{x}{y}` (id
    `name{}`) for each name with the audit test. Returns (outputs, failures),
    both keyed by id."""
    with tempfile.TemporaryDirectory() as tmp:
        corpus = Path(tmp) / "probes.json"
        report = Path(tmp) / "report.jsonl"
        outputs = Path(tmp) / "outputs.jsonl"
        corpus.write_text(json.dumps([{"id": i, "latex": f"\\{n}{a}"} for n in names for i, a in ((n, ""), (n + "{}", "{x}{y}"))]), encoding="utf-8")
        env = dict(os.environ, TYLAX_CORPUS=str(corpus), TYLAX_CORPUS_REPORT=str(report), TYLAX_CORPUS_OUTPUTS=str(outputs))
        command = ["cargo", "test", "--release", "--features", "data-loading", "--test", "corpus_compile_audit",
                   "--", "--ignored", "--nocapture"]
        result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
        if result.returncode != 0:
            sys.exit(result.stderr[-3000:])
        read = lambda path: {r["id"]: r for r in map(json.loads, path.read_text(encoding="utf-8").splitlines())}
        return read(outputs), read(report)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--corpus", nargs="*", default=[], help="corpora for command frequencies")
    parser.add_argument("--commands", help="output of tools/latex_command_lists.mjs (default: run it)")
    parser.add_argument("--unicode-math", help="path to unicode-math-table.tex (default: kpsewhich)")
    parser.add_argument("--top", type=int, default=0, help="print only the N most frequent unmapped commands")
    args = parser.parse_args()

    katex, mathjax = katex_mathjax(args.commands)
    umath = unicode_math(args.unicode_math)
    sources = {}
    for name in katex:
        sources.setdefault(name, set()).add("K")
    for name in mathjax:
        sources.setdefault(name, set()).add("M")
    for name in umath:
        sources.setdefault(name, set()).add("U")
    names = sorted(n for n in sources if n not in STRUCTURAL)

    counts = corpus_counts(args.corpus)
    outputs, failures = probe(names)
    literals = known_literals()

    def row(name: str, key: str, note: str) -> str:
        src = "".join(s for s in "KMU" if s in sources[name])
        typst = (outputs.get(key) or {}).get("typst")
        char = f" {umath[name][0]}" if name in umath else ""
        return f"{counts[name]:7}  {src:3}  \\{name}{char}  ->  {typst}  [{note}]"

    def first_error(key: str) -> str:
        return failures[key]["errors"][0]["message"]

    by_frequency = lambda n: (-counts[n], n)
    limit = args.top or None
    unmapped = sorted((n for n in names if n in failures and n + "{}" in failures), key=by_frequency)
    glued = sorted((n for n in names if n not in failures and n + "{}" in failures), key=by_frequency)

    print(f"== unmapped: {len(unmapped)} of {len(names)} commands compile neither alone nor before a group ==")
    for source, label in (("K", "KaTeX"), ("M", "MathJax"), ("U", "unicode-math")):
        total = sum(1 for n in names if source in sources[n])
        bad = sum(1 for n in unmapped if source in sources[n])
        print(f"   {label}: {bad} of {total}")
    classes = Counter(umath[n][1] for n in unmapped if n in umath)
    print("   unicode-math classes: " + ", ".join(f"{c} {k}" for k, c in classes.most_common()))
    for name in unmapped[:limit]:
        print(row(name, name, first_error(name)))

    print(f"\n== compile alone, but not before a group (output glues to the next token): {len(glued)} ==")
    for name in glued[:limit]:
        print(row(name, name + "{}", first_error(name + "{}")))

    accidental = []
    for name in names:
        typst = (outputs.get(name) or {}).get("typst") or ""
        if name in failures or name in literals:
            continue
        if re.search(rf"(?<![\\w.]){re.escape(name)}(?![\\w.])", typst):
            accidental.append(name)
    print(f"\n== pass through as the bare name but compile: {len(accidental)} (check the meaning) ==")
    for name in sorted(accidental, key=by_frequency)[:limit]:
        print(row(name, name, "bare name"))


if __name__ == "__main__":
    main()
