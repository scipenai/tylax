"""Group the crash artifacts of one fuzz target by failure.

Run from the repository root after `cargo fuzz build`:

    python3 -I fuzz/triage.py <target> [examples-per-group]

Each artifact is replayed through the target binary. For l2t_math_parses the
group key is the first Typst parse error; for the other targets it is the panic
location or the sanitizer summary. The shortest inputs in each group are shown.
"""

import collections
import glob
import platform
import re
import subprocess
import sys
from pathlib import Path

target = sys.argv[1]
per_group = int(sys.argv[2]) if len(sys.argv) > 2 else 3

fuzz = Path(__file__).resolve().parent
triple = f"{'aarch64' if platform.machine() == 'arm64' else platform.machine()}-apple-darwin" \
    if sys.platform == "darwin" else f"{platform.machine()}-unknown-linux-gnu"
binary = fuzz / "target" / triple / "release" / target

groups = collections.defaultdict(list)
for path in sorted(glob.glob(str(fuzz / "artifacts" / target / "*"))):
    name = Path(path).name
    if name.startswith("minimized-from-"):
        continue
    try:
        # Match run.sh: AddressSanitizer needs a larger stack than the 8 MB default.
        run = subprocess.run(["/bin/sh", "-c", 'ulimit -s 65520 && exec "$0" "$1"', binary, path],
                             capture_output=True, text=True,
                             errors="replace", timeout=30)
        log, failed = run.stderr, run.returncode != 0
    except subprocess.TimeoutExpired:
        log, failed = "timeout", True
    if errors := re.search(r'^errors: \["([^"]*)"', log, re.M):
        key = errors.group(1)
    elif panic := re.search(r"panicked at (\S+)", log):
        key = f"panic {panic.group(1)}"
    elif summary := re.search(r"SUMMARY: (.*?)(?: \(|$)", log, re.M):
        key = summary.group(1)
    elif not failed:
        key = "did not reproduce"
    elif log == "timeout" or name.startswith(("timeout-", "slow-unit-")):
        key = "timeout"
    elif name.startswith("oom-"):
        key = "oom"
    else:
        key = "did not reproduce"
    data = Path(path).read_bytes()
    groups[key].append((len(data), data, path))

for key, cases in sorted(groups.items(), key=lambda kv: -len(kv[1])):
    print(f"## {key} ({len(cases)})")
    for _, data, path in sorted(cases)[:per_group]:
        print(f"  {Path(path).name}: {data[:160]!r}")
