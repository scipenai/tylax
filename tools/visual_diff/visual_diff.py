#!/usr/bin/env python3
"""Visual diff of LaTeX math against tylax's Typst output.

Renders each LaTeX formula with MathJax (SVG, rasterised by rsvg-convert) and
its `tylax::latex_to_typst` output with `typst compile`, both in display style
at the same pixels per em. Both images are then normalised the same way:

1. greyscale, and trim the white border;
2. scale the Typst image to the height of the MathJax image (the reference);
3. pad the narrower image on the right with white to the same width, so both
   are left-aligned;
4. scale both to `--height` pixels high and blur them slightly, so that a
   shift of a pixel or two from different font metrics does not dominate.

`ssim` is SSIM from `magick compare` on those images (1 is identical).
`ssim_stretch` stretches the Typst image to the width of the MathJax image
instead of padding it, which forgives uniform spacing differences. Before any
scaling, `width_ratio` and `height_ratio` compare the trimmed sizes (both are
rendered at the same pixels per em), and `aspect` is the Typst width-to-height
ratio over the MathJax one. `score`, the default ranking for review, is
`ssim_stretch - 0.5 * |ln width_ratio|`: dropped or shrunk content shows in
the width before it shows in SSIM. No value separates real bugs from rendering
noise; the scores only order the pairs for review by eye.

Usage (standard library only; needs cargo, node, typst, rsvg-convert, magick,
and MathJax 3 installed outside the repository:
`npm install --prefix ~/.cache/tylax/node mathjax-full@3.2.2`):

    python3 -I tools/visual_diff/visual_diff.py run INPUT --out DIR [--sample N]
    python3 -I tools/visual_diff/visual_diff.py sheets DIR [--worst N]

INPUT is a JSON array or JSONL of {"id", "latex"} objects. `run` writes
DIR/results.jsonl and prints the score distribution. `sheets` writes contact
sheets of the worst pairs to DIR/sheets/ (MathJax above, Typst below) for
review by eye. Keep DIR out of the repository (for example under /tmp).
"""

import argparse
import concurrent.futures
import glob
import json
import math
import os
import random
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
LABEL_FONT_CANDIDATES = (
    "/System/Library/Fonts/Menlo.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    r"C:\Windows\Fonts\consola.ttf",
)

# Rendering scale: 40 px per em. Typst text is 10 pt, so 288 ppi matches.
PX_PER_EM = 40
TYPST_PT = 10
TYPST_PPI = PX_PER_EM * 72 / TYPST_PT


def driver_path():
    target_dir = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
    if not os.path.isabs(target_dir):
        target_dir = os.path.join(REPO, target_dir)
    name = "l2t_batch.exe" if os.name == "nt" else "l2t_batch"
    return os.path.join(target_dir, "release", "examples", name)


def find_label_font():
    override = os.environ.get("TYLAX_LABEL_FONT")
    if override:
        return override
    for candidate in LABEL_FONT_CANDIDATES:
        if os.path.isfile(candidate):
            return candidate
    return None


def require_tools(names):
    missing = [tool for tool in names if shutil.which(tool) is None]
    if missing:
        sys.exit("missing required tools on PATH: " + ", ".join(missing))


def run(cmd, **kwargs):
    return subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        **kwargs,
    )


def load_entries(path):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    if text.lstrip().startswith("["):
        entries = json.loads(text)
    else:
        entries = [json.loads(line) for line in text.splitlines() if line.strip()]
    return [{"id": str(e.get("id", i)), "latex": e["latex"]} for i, e in enumerate(entries)]


def write_jsonl(path, records):
    with open(path, "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")


def read_jsonl(path):
    with open(path, encoding="utf-8") as f:
        return [json.loads(line) for line in f if line.strip()]


def convert(entries, out, build):
    if build:
        res = run(["cargo", "build", "--release", "--example", "l2t_batch"], cwd=REPO)
        if res.returncode != 0:
            sys.exit("cargo build failed:\n" + res.stderr)
    src = os.path.join(out, "input.jsonl")
    write_jsonl(src, entries)
    res = run([driver_path(), src])
    if res.returncode != 0:
        sys.exit("l2t_batch failed:\n" + res.stderr)
    records = [json.loads(line) for line in res.stdout.splitlines() if line.strip()]
    assert len(records) == len(entries)
    return records


def render_mathjax(out):
    svg_dir = os.path.join(out, "mathjax")
    res = run(["node", os.path.join(HERE, "mathjax_render.js"),
               os.path.join(out, "converted.jsonl"), svg_dir, str(PX_PER_EM)])
    if res.returncode != 0:
        sys.exit("mathjax_render.js failed:\n" + res.stderr)
    return [json.loads(line) for line in res.stdout.splitlines() if line.strip()]


def typst_source(formulas):
    """One page per formula, with top-level equations rendered in display style.
    Returns the source and, per formula, its 1-based line range."""
    lines = ["#set page(width: auto, height: auto, margin: 2pt, fill: white)",
             f"#set text(size: {TYPST_PT}pt)",
             "#let tylax-display(eq) = if eq.func() == math.equation {",
             "  math.equation(eq.body, block: true)",
             "} else {",
             "  eq",
             "}"]
    ranges = []
    for n, typst in enumerate(formulas):
        if n:
            lines.append("#pagebreak()")
        stripped = typst.strip()
        body = stripped.split("\n")
        start = len(lines) + 1
        if stripped.startswith("$"):
            end = next_unescaped_dollar(stripped)
            if end is None:
                lines.extend(body)
            else:
                lines.extend(("#tylax-display[" + stripped[:end] + "]" + stripped[end:]).split("\n"))
        elif stripped.startswith("#math.equation"):
            lines.extend(body)
        else:
            lines.append("$ " + body[0])
            lines.extend(body[1:])
            lines.append("$")
        ranges.append((start, len(lines)))
    return "\n".join(lines) + "\n", ranges


def next_unescaped_dollar(text):
    escaped = False
    for index in range(1, len(text)):
        char = text[index]
        if char == "$" and not escaped:
            return index + 1
        if char == "\\" and not escaped:
            escaped = True
        else:
            escaped = False
    return None


ERROR_LINE = re.compile(r"^(.*?):(\d+):\d+: error: (.*)$")


def compile_batch(work, name, formulas):
    """Compile one batch to PNG pages. A formula that does not compile is
    replaced by an empty equation and the batch is compiled again. Blamed
    entries are confirmed singly so an unclosed token in one formula cannot
    falsely condemn a neighbour. Returns the page paths and the error message
    per formula (None when it compiled)."""
    formulas = list(formulas)
    errors = [None] * len(formulas)
    typ = os.path.join(work, name + ".typ")
    try:
        for _ in range(len(formulas) + 1):
            source, ranges = typst_source(formulas)
            with open(typ, "w", encoding="utf-8") as f:
                f.write(source)
            for old in glob.glob(os.path.join(work, name + "-*.png")):
                os.remove(old)
            try:
                res = run(["typst", "compile", "--diagnostic-format", "short",
                           "--ppi", str(TYPST_PPI), typ,
                           os.path.join(work, name + "-{0p}.png")], timeout=600)
            except subprocess.TimeoutExpired:
                res = None
            if res is not None and res.returncode == 0:
                pages = sorted(glob.glob(os.path.join(work, name + "-*.png")),
                               key=lambda p: int(p.rsplit("-", 1)[1][:-4]))
                assert len(pages) == len(formulas), (name, len(pages), len(formulas))
                return pages, errors
            culprits = {}
            if res is not None:
                for line in res.stderr.splitlines():
                    m = ERROR_LINE.match(line)
                    if not m or not m.group(1).endswith(name + ".typ"):
                        continue
                    lineno = int(m.group(2))
                    for n, (a, b) in enumerate(ranges):
                        if a <= lineno <= b and errors[n] is None:
                            culprits.setdefault(n, m.group(3))
            confirmed = False
            for n in culprits:
                try:
                    p, err = compile_batch_once(work, f"{name}c{n}", formulas[n])
                except subprocess.TimeoutExpired:
                    p, err = None, "typst timed out"
                if err is not None:
                    errors[n] = err
                    formulas[n] = ""
                    confirmed = True
                elif p and os.path.exists(p):
                    os.remove(p)
            if not confirmed:
                # The error could not be pinned on one formula (or typst timed
                # out): compile each remaining formula alone.
                return compile_singly(work, name, formulas, errors)
        raise RuntimeError("typst batch did not converge: " + name)
    finally:
        if os.path.exists(typ):
            os.remove(typ)


def compile_singly(work, name, formulas, errors):
    pages = []
    for n, typst in enumerate(formulas):
        sub = f"{name}s{n}"
        p = None
        if errors[n] is None:
            try:
                p, e = compile_batch_once(work, sub, typst)
            except subprocess.TimeoutExpired:
                p, e = None, "typst timed out"
            if e is not None:
                errors[n] = e
        pages.append(p)
    return pages, errors


def compile_batch_once(work, name, typst):
    source, _ = typst_source([typst])
    typ = os.path.join(work, name + ".typ")
    png = os.path.join(work, name + "-1.png")
    with open(typ, "w", encoding="utf-8") as f:
        f.write(source)
    try:
        res = run(["typst", "compile", "--diagnostic-format", "short",
                   "--ppi", str(TYPST_PPI), typ, png], timeout=120)
    finally:
        if os.path.exists(typ):
            os.remove(typ)
    if res.returncode != 0:
        if os.path.exists(png):
            os.remove(png)
        first = next((l for l in res.stderr.splitlines() if ": error: " in l), res.stderr)
        return None, first.split(": error: ", 1)[-1].strip()
    return png, None


def render_typst(records, out, batch, jobs):
    work = os.path.join(out, "typst")
    os.makedirs(work, exist_ok=True)
    todo = [(i, r) for i, r in enumerate(records) if r.get("typst") is not None]
    chunks = [todo[k:k + batch] for k in range(0, len(todo), batch)]
    results = {}

    def one(c, chunk):
        pages, errors = compile_batch(work, f"b{c:04d}", [r["typst"] for _, r in chunk])
        return [(i, page, err) for (i, _), page, err in zip(chunk, pages, errors)]

    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        for part in pool.map(lambda a: one(*a), enumerate(chunks)):
            for i, page, err in part:
                target = os.path.join(work, f"{i:05d}.png")
                if err is None:
                    shutil.move(page, target)
                results[i] = err
    return results


def magick_trim(src, dst):
    """Greyscale and trim; returns (width, height), or None for a blank image."""
    # The darkest pixel is measured before trimming: trimming a blank image
    # leaves one transparent pixel.
    res = run(["magick", src, "-background", "white", "-flatten", "-colorspace", "Gray",
               "-format", "%[fx:minima] ", "-write", "info:", "-trim", "+repage",
               "-format", "%w %h\n", "-write", "info:", dst])
    if res.returncode != 0:
        return None
    lo, w, h = res.stdout.split()
    if float(lo) > 0.9:
        return None
    return int(w), int(h)


def ssim(a_args, b_args):
    res = run(["magick", "(", *a_args, ")", "(", *b_args, ")",
               "-metric", "SSIM", "-compare", "-format", "%[distortion]", "info:"])
    text = res.stdout.strip()
    if res.returncode not in (0, 1) or not text:
        raise RuntimeError(res.stderr or f"magick exited with {res.returncode}")
    try:
        distortion = float(text)
    except ValueError as err:
        raise RuntimeError(f"unexpected magick SSIM output {text!r}: {res.stderr}") from err
    # ImageMagick 7.1.2 reports 1 - SSIM as the distortion.
    return round(1.0 - distortion, 4)


def compare_pair(out, i, height, blur):
    mj_svg = os.path.join(out, "mathjax", f"{i:05d}.svg")
    mj_png = os.path.join(out, "mathjax", f"{i:05d}.png")
    ty_png = os.path.join(out, "typst", f"{i:05d}.png")
    norm = os.path.join(out, "norm")
    res = run(["rsvg-convert", "-b", "white", mj_svg, "-o", mj_png])
    if res.returncode != 0:
        return {"status": "rsvg_error"}
    m_trim = os.path.join(norm, f"{i:05d}-m.png")
    t_trim = os.path.join(norm, f"{i:05d}-t.png")
    m = magick_trim(mj_png, m_trim)
    if m is None:
        return {"status": "latex_blank"}
    t = magick_trim(ty_png, t_trim)
    if t is None:
        return {"status": "typst_blank"}
    (wm, hm), (wt, ht) = m, t
    # Scale the Typst image to the MathJax height, then pad both to one width.
    wt_scaled = max(1, round(wt * hm / ht))
    width = max(wm, wt_scaled)
    common = ["-background", "white", "-gravity", "West", "-extent", f"{width}x{hm}"]
    final = ["-resize", f"x{height}"] + (["-blur", f"0x{blur}"] if blur else [])
    a = [m_trim] + common + final
    b = [t_trim, "-resize", f"{wt_scaled}x{hm}!"] + common + final
    stretched = [t_trim, "-resize", f"{wm}x{hm}!", "-resize", f"x{height}"] + (
        ["-blur", f"0x{blur}"] if blur else [])
    a_plain = [m_trim, "-resize", f"x{height}"] + (["-blur", f"0x{blur}"] if blur else [])
    s_stretch = ssim(a_plain, stretched)
    return {
        "status": "ok",
        "ssim": ssim(a, b),
        "ssim_stretch": s_stretch,
        "aspect": round((wt / ht) / (wm / hm), 3),
        "height_ratio": round(ht / hm, 3),
        "width_ratio": round(wt / wm, 3),
        "score": round(s_stretch - 0.5 * abs(math.log(wt / wm)), 4),
        "size_m": [wm, hm],
        "size_t": [wt, ht],
    }


def quantiles(values, qs=(0.0, 0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0)):
    v = sorted(values)
    if not v:
        return {}
    return {q: v[min(len(v) - 1, int(q * (len(v) - 1) + 0.5))] for q in qs}


def cmd_run(args):
    required = ([] if args.no_build else ["cargo"]) + ["node", "typst", "rsvg-convert", "magick"]
    require_tools(required)
    entries = load_entries(args.input)
    if args.sample and args.sample < len(entries):
        entries = random.Random(args.seed).sample(entries, args.sample)

    out = os.path.abspath(args.out)
    for sub in ("mathjax", "typst", "norm"):
        shutil.rmtree(os.path.join(out, sub), ignore_errors=True)
        os.makedirs(os.path.join(out, sub), exist_ok=True)
    print(f"{len(entries)} formulas", file=sys.stderr)

    records = convert(entries, out, not args.no_build)
    write_jsonl(os.path.join(out, "converted.jsonl"), records)
    mj = render_mathjax(out)
    ty_errors = render_typst(records, out, args.batch, args.jobs)
    print("rendered; comparing", file=sys.stderr)

    def one(i):
        r = records[i]
        base = {"index": i, "id": r["id"], "latex": r["latex"], "typst": r["typst"]}
        if r["typst"] is None:
            return {**base, "status": "convert_panic", "error": r.get("error")}
        if not mj[i]["ok"]:
            return {**base, "status": "latex_error", "error": mj[i]["error"]}
        if ty_errors.get(i):
            return {**base, "status": "typst_error", "error": ty_errors[i]}
        return {**base, **compare_pair(out, i, args.height, args.blur)}

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(one, range(len(records))))
    write_jsonl(os.path.join(out, "results.jsonl"), results)
    summarise(results)


def summarise(results):
    counts = {}
    for r in results:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    print("status:", ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    ok = [r for r in results if r["status"] == "ok"]
    for key in ("score", "ssim", "ssim_stretch", "width_ratio", "aspect"):
        q = quantiles([r[key] for r in ok])
        print(f"{key:13}", "  ".join(f"q{int(k * 100):<3}{v:.3f}" for k, v in q.items()))
    print("worst by score (score, ssim, ssim_stretch, width_ratio):")
    for r in sorted(ok, key=lambda r: r["score"])[:15]:
        print(f"  {r['score']:.3f} {r['ssim']:.3f} {r['ssim_stretch']:.3f} {r['width_ratio']:.2f}"
              f"  {r['id']}  {r['latex'][:70]}")


def cmd_sheets(args):
    require_tools(["magick"])
    out = os.path.abspath(args.dir)
    results = [r for r in read_jsonl(os.path.join(out, "results.jsonl")) if r["status"] == "ok"]
    if args.key == "aspect":
        # Furthest from 1 first, in either direction.
        results.sort(key=lambda r: -abs(math.log(r["aspect"])))
    else:
        results.sort(key=lambda r: r[args.key])
    if args.start:
        results = results[args.start:]
    results = results[: args.worst]
    sheets = os.path.join(out, "sheets")
    shutil.rmtree(sheets, ignore_errors=True)
    os.makedirs(sheets)
    width = args.width
    font = find_label_font()
    font_args = ["-font", font] if font else []

    def tile(r):
        i = r["index"]
        label = (f"#{r['index']} {r['id']}  score {r['score']:.3f}  ssim {r['ssim']:.3f}"
                 f"  stretch {r['ssim_stretch']:.3f}  width {r['width_ratio']:.2f}")
        safe_label = label.replace("%", "%%")
        if safe_label.startswith("@"):
            safe_label = "\\" + safe_label
        dst = os.path.join(sheets, f"tile-{i:05d}.png")
        # One scale for both images, so that their sizes stay comparable.
        (wm, hm), (wt, ht) = r["size_m"], r["size_t"]
        scale = min(1.0, width / max(wm, wt), args.max_height / max(hm, ht))
        fit = ["-resize", f"{scale * 100:.2f}%", "-background", "white",
               "-gravity", "West", "-extent", f"{width}x"]
        res = run(["magick",
                   "(", "-size", f"{width}x22", "xc:#ddeeff", *font_args,
                   "-pointsize", "14", "-gravity", "West", "-annotate", "+4+0", safe_label, ")",
                   "(", os.path.join(out, "norm", f"{i:05d}-m.png"), *fit, ")",
                   "(", "-size", f"{width}x2", "xc:#ffaaaa", ")",
                   "(", os.path.join(out, "norm", f"{i:05d}-t.png"), *fit, ")",
                   "(", "-size", f"{width}x8", "xc:white", ")",
                   "-append", dst])
        if res.returncode != 0:
            raise RuntimeError(res.stderr)
        return dst

    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        tiles = list(pool.map(tile, results))
    per = args.per_sheet
    for s in range(0, len(tiles), per):
        name = os.path.join(sheets, f"sheet-{s // per:03d}.png")
        res = run(["magick", *tiles[s:s + per], "-append", name])
        if res.returncode != 0:
            raise RuntimeError(res.stderr or f"magick exited with {res.returncode}")
        print(name)
        for r in results[s:s + per]:
            print(f"  #{r['index']} {r['id']}: {r['latex']}\n      -> {r['typst']}")
    for t in tiles:
        os.remove(t)


def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="convert, render and score a formula list")
    r.add_argument("input")
    r.add_argument("--out", required=True, help="scratch directory for images and results")
    r.add_argument("--sample", type=int, default=0, help="random sample size (0: all)")
    r.add_argument("--seed", type=int, default=1)
    r.add_argument("--batch", type=int, default=200, help="formulas per typst document")
    r.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    r.add_argument("--height", type=int, default=48, help="normalised height in pixels")
    r.add_argument("--blur", type=float, default=1.0, help="Gaussian sigma after scaling")
    r.add_argument("--no-build", action="store_true", help="skip cargo build")
    s = sub.add_parser("sheets", help="contact sheets of the worst pairs")
    s.add_argument("dir")
    s.add_argument("--worst", type=int, default=60)
    s.add_argument("--start", type=int, default=0, help="skip this many of the worst")
    s.add_argument("--key", default="score",
                   choices=["score", "ssim", "ssim_stretch", "aspect"])
    s.add_argument("--per-sheet", type=int, default=6)
    s.add_argument("--width", type=int, default=900)
    s.add_argument("--max-height", type=int, default=120)
    args = p.parse_args()
    {"run": cmd_run, "sheets": cmd_sheets}[args.cmd](args)


if __name__ == "__main__":
    main()
