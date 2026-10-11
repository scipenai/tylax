"""Write fuzz seed files from tests/corpus_merged.json.

Run from the repository root: python3 -I fuzz/make_seeds.py
"""

import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent
cases = json.loads((root / "tests" / "corpus_merged.json").read_text())

for field, out in (("latex", "latex_math"), ("typst", "typst_math")):
    out_dir = root / "fuzz" / "seeds" / out
    out_dir.mkdir(parents=True, exist_ok=True)
    for case in cases:
        text = case.get(field)
        if not text:
            continue
        data = text.encode()
        (out_dir / hashlib.sha1(data).hexdigest()).write_bytes(data)
