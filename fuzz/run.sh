#!/bin/sh
# Run one fuzz target with its dictionary and seeds.
#
# Usage: fuzz/run.sh <target> [seconds]   (run from the repository root)
# Crashes and timeouts go to fuzz/artifacts/<target>/; the run carries on after each one.
# AddressSanitizer's larger stack frames overflow the default 8 MB stack on input
# a normal build handles, so the stack is raised to the macOS maximum. (Turning
# the sanitizer off fails to link, because tylax also builds as a cdylib.)
set -eu
target=$1
seconds=${2:-300}
case $target in
  l2t_math|l2t_math_parses) dict=latex; seeds=latex_math ;;
  l2t_document) dict=latex; seeds=latex_document ;;
  t2l_math) dict=typst; seeds=typst_math ;;
  t2l_document) dict=typst; seeds=typst_document ;;
  *) echo "unknown target: $target" >&2; exit 2 ;;
esac
[ -d "fuzz/seeds/$seeds" ] || python3 -I fuzz/make_seeds.py
mkdir -p "fuzz/corpus/$target"
ulimit -s 65520
cd fuzz
exec cargo fuzz run "$target" "corpus/$target" "seeds/$seeds" -- \
  -dict="dict/$dict.dict" -max_total_time="$seconds" -timeout=5 -max_len=4096 \
  -fork=1 -ignore_crashes=1 -ignore_timeouts=1 -ignore_ooms=1
