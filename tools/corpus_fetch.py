#!/usr/bin/env python3
"""Download third-party LaTeX math corpora for the compile audit.

The corpora stay out of the repository, because their licences vary. Each one
goes under ~/.cache/tylax/corpora/<name>/: the downloaded files in raw/, and
the extracted formulas in formulas.json as a JSON array of {"id", "latex"},
deduplicated. Filter them with tools/corpus_katex_filter.mjs before the audit.

Usage:
    python3 -I tools/corpus_fetch.py im2latex
    python3 -I tools/corpus_fetch.py mse [--pages 25]

im2latex: the im2latex-100k formula list (Zenodo record 56198), about 100,000
formulas from arXiv papers. `\\label{...}` is removed, since KaTeX rejects it
and it is not part of the formula.

mse: questions and answers from Math StackExchange, highest voted first, via
the Stack Exchange API (no key: 300 requests a day). Each page holds 100 posts;
--pages applies to questions and to answers. Math is taken from $...$ and
$$...$$ outside code blocks.
"""

import argparse
import gzip
import html
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

CACHE = Path.home() / ".cache" / "tylax" / "corpora"
IM2LATEX_URL = "https://zenodo.org/records/56198/files/im2latex_formulas.lst?download=1"
SE_API = "https://api.stackexchange.com/2.3"
USER_AGENT = "tylax-corpus-fetch/0.1"


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept-Encoding": "gzip"})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
        if response.headers.get("Content-Encoding") == "gzip":
            data = gzip.decompress(data)
        return data


def write_formulas(name: str, formulas: list[tuple[str, str]]) -> None:
    seen = set()
    unique = []
    for case_id, latex in formulas:
        latex = latex.strip()
        if latex and latex not in seen:
            seen.add(latex)
            unique.append({"id": case_id, "latex": latex})
    out = CACHE / name / "formulas.json"
    out.write_text(json.dumps(unique, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{name}: {len(formulas)} formulas, {len(unique)} unique -> {out}")


LABEL = re.compile(r"\\label\s*\{[^{}]*\}")


def im2latex(_args: argparse.Namespace) -> None:
    raw = CACHE / "im2latex" / "raw"
    raw.mkdir(parents=True, exist_ok=True)
    path = raw / "im2latex_formulas.lst"
    if not path.exists():
        print(f"downloading {IM2LATEX_URL}")
        path.write_bytes(fetch(IM2LATEX_URL))
    # Formulas are separated by "\n" only; a bare "\r" can occur inside one.
    text = path.read_bytes().decode("utf-8", errors="replace")
    formulas = []
    for number, line in enumerate(text.split("\n")):
        latex = LABEL.sub("", line)
        latex = re.sub(r"[\r\t]", " ", latex)
        formulas.append((f"im2latex:{number}", latex))
    write_formulas("im2latex", formulas)


CODE = re.compile(r"<(pre|code)\b.*?</\1>", re.DOTALL)
DISPLAY = re.compile(r"\$\$(.+?)\$\$", re.DOTALL)
INLINE = re.compile(r"(?<![\\$])\$([^$]+?)(?<!\\)\$")


def extract_math(body: str) -> list[str]:
    """Math from a post's HTML body. A formula holding a tag spans markup
    that the renderer added, so it is skipped; `%` would comment out the rest
    of the line once newlines are flattened, so such formulas are skipped too."""
    body = CODE.sub(" ", body)
    found = DISPLAY.findall(body)
    found += INLINE.findall(DISPLAY.sub(" ", body))
    result = []
    for latex in found:
        if "<" in latex or ">" in latex:
            continue
        latex = html.unescape(latex)
        if "%" in latex.replace("\\%", ""):
            continue
        result.append(" ".join(latex.split()))
    return result


def mse(args: argparse.Namespace) -> None:
    raw = CACHE / "mse" / "raw"
    raw.mkdir(parents=True, exist_ok=True)
    formulas = []
    for kind in ("questions", "answers"):
        for page in range(1, args.pages + 1):
            path = raw / f"{kind}-{page:03}.json"
            if not path.exists():
                query = urllib.parse.urlencode(
                    {"site": "math", "sort": "votes", "order": "desc", "pagesize": 100, "page": page, "filter": "withbody"}
                )
                data = json.loads(fetch(f"{SE_API}/{kind}?{query}"))
                if "error_id" in data:
                    sys.exit(f"Stack Exchange API error: {data}")
                path.write_text(json.dumps(data), encoding="utf-8")
                print(f"{kind} page {page}: quota remaining {data.get('quota_remaining')}")
                time.sleep(max(data.get("backoff", 0), 0.2))
            data = json.loads(path.read_text(encoding="utf-8"))
            for item in data.get("items", []):
                post_id = item.get("answer_id") or item.get("question_id")
                for number, latex in enumerate(extract_math(item.get("body", ""))):
                    formulas.append((f"mse:{post_id}:{number}", latex))
            if not data.get("has_more", False):
                break
    write_formulas("mse", formulas)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("im2latex").set_defaults(run=im2latex)
    mse_parser = commands.add_parser("mse")
    mse_parser.add_argument("--pages", type=int, default=25, help="pages of 100 posts each (default 25)")
    mse_parser.set_defaults(run=mse)
    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
