#!/usr/bin/env python3
"""Group the failures in a compile-audit report before triage.

Reads one or more JSONL reports written by tests/corpus_compile_audit.rs
(TYLAX_CORPUS_REPORT) and prints two groupings, largest first, with the
shortest examples of each:

- by error template: the first Typst error, with names, numbers and quoted
  code replaced by placeholders;
- by culprit: the name Typst did not know (`unknown variable: dbinom`), or the
  code at the error column for other errors, together with the first LaTeX
  command whose name starts that unknown name (`\\dbinom`).

Usage:
    python3 -I tools/corpus_failure_groups.py REPORT.jsonl [...] [--top 30] [--examples 3]
"""

import argparse
import json
import re
from collections import defaultdict

NAME = re.compile(r"[A-Za-z][A-Za-z0-9-]*(?:\.[A-Za-z][A-Za-z0-9-]*)*")
COMMAND = re.compile(r"\\([A-Za-z]+)")


def template(message: str) -> str:
    message = re.sub(r"`[^`]*`", "`…`", message)
    message = re.sub(r"(unknown variable|missing argument|unexpected argument): .*", r"\1: <name>", message)
    message = re.sub(r"\b\d+(\.\d+)?\b", "N", message)
    return message


def token_at(typst: str, column) -> str:
    """The dotted name or single character at `column` of `typst`."""
    # No column: the error is on the enclosing `$`, usually an unclosed delimiter.
    if column is None:
        return "<math delimiter>"
    if column >= len(typst):
        return "<end>"
    start = column
    while start > 0 and (typst[start - 1].isalnum() or typst[start - 1] in ".-"):
        start -= 1
    match = NAME.match(typst, start)
    if match and match.end() > column:
        return match.group(0)
    return typst[column]


def culprit(record: dict) -> str:
    error = record["errors"][0]
    message = error["message"]
    typst = record.get("typst") or ""
    if message.startswith("unknown variable: "):
        name = message.split(": ", 1)[1]
    else:
        name = token_at(typst, error.get("column"))
    head = name.split(".")[0]
    command = next(
        (c for c in COMMAND.findall(record["latex"]) if len(head) >= 2 and head.startswith(c)),
        None,
    )
    return f"{name}  (\\{command})" if command else name


def first_error(record: dict) -> str:
    return record["errors"][0]["message"] if record["errors"] else "<none>"


def print_groups(title: str, groups: dict, top: int, examples: int) -> None:
    total = sum(len(v) for v in groups.values())
    print(f"== {title}: {len(groups)} groups, {total} failures ==")
    for key, records in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0]))[:top]:
        print(f"\n{len(records):6}  {key}")
        for record in sorted(records, key=lambda r: (len(r["latex"]), r["id"]))[:examples]:
            print(f"        {record['id']}: {record['latex']}")
            print(f"          -> {record.get('typst')}")
            print(f"          !! {first_error(record)}")
    print()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("reports", nargs="+")
    parser.add_argument("--top", type=int, default=30, help="groups to print per grouping (default 30)")
    parser.add_argument("--examples", type=int, default=3, help="examples per group (default 3)")
    args = parser.parse_args()

    records = []
    for path in args.reports:
        with open(path, encoding="utf-8") as report:
            records += [json.loads(line) for line in report if line.strip()]

    by_template = defaultdict(list)
    by_culprit = defaultdict(list)
    for record in records:
        if not record["errors"]:
            continue
        by_template[template(first_error(record))].append(record)
        by_culprit[culprit(record)].append(record)

    print_groups("by error template", by_template, args.top, args.examples)
    print_groups("by culprit", by_culprit, args.top, args.examples)


if __name__ == "__main__":
    main()
