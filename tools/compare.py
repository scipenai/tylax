#!/usr/bin/env python3
"""
Tylax vs Pandoc Comparison Tool

Usage:
    python compare.py                    # Run all tests
    python compare.py --math "\\frac{1}{2}"  # Test single math expression
    python compare.py --file input.tex   # Test file
    python compare.py --benchmark        # Run performance benchmark
"""

import subprocess
import sys
import time
import json
import argparse
from pathlib import Path

# Test cases
MATH_TEST_CASES = [
    # Basic
    (r"\frac{1}{2}", "Basic fraction"),
    (r"\frac{1}{2}_3", "Fraction with subscript (edge case)"),
    (r"\sqrt{x^2 + y^2}", "Square root"),
    (r"\sqrt[3]{x}", "Nth root"),

    # Greek
    (r"\alpha + \beta = \gamma", "Greek letters"),
    (r"\Gamma \Delta \Theta", "Uppercase Greek"),

    # Operators
    (r"\sum_{i=1}^{n} i^2", "Summation"),
    (r"\sum\limits_{i=1}^{n} i", "Sum with limits"),
    (r"\int_0^\infty e^{-x} dx", "Integral"),
    (r"\prod_{i=1}^{n} x_i", "Product"),

    # Relations
    (r"a \leq b \geq c \neq d", "Comparisons"),
    (r"A \subset B \subseteq C", "Set relations"),

    # Matrices
    (r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", "Matrix"),
    (r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}", "Bracket matrix"),

    # Complex
    (r"\frac{\partial^2 u}{\partial x^2} + \frac{\partial^2 u}{\partial y^2} = 0", "Laplace equation"),
    (r"e^{i\pi} + 1 = 0", "Euler's identity"),
    (r"\lim_{n \to \infty} \left(1 + \frac{1}{n}\right)^n = e", "Limit of e"),
]

DOCUMENT_TEST = r"""
\documentclass{article}
\title{Test Document}
\author{Test Author}
\begin{document}
\maketitle
\section{Introduction}
This is a test with math: $E = mc^2$.

\subsection{Details}
A list:
\begin{itemize}
\item First
\item Second
\end{itemize}

An equation:
\begin{equation}
\int_0^\infty e^{-x} dx = 1
\end{equation}
\end{document}
"""


def run_tylax(input_text: str, mode: str = "math") -> tuple[str, float]:
    """Run Tylax conversion"""
    start = time.perf_counter()
    try:
        if mode == "math":
            cmd = ["cargo", "run", "--release", "--features", "cli", "--", "-d", "l2t"]
        else:
            cmd = ["cargo", "run", "--release", "--features", "cli", "--", "-d", "l2t", "-f"]

        result = subprocess.run(
            cmd,
            input=input_text,
            capture_output=True,
            text=True,
            cwd=Path(__file__).parent,
            timeout=30
        )
        elapsed = (time.perf_counter() - start) * 1000
        return result.stdout.strip(), elapsed
    except Exception as e:
        elapsed = (time.perf_counter() - start) * 1000
        return f"Error: {e}", elapsed


def run_pandoc(input_text: str) -> tuple[str, float]:
    """Run Pandoc conversion"""
    start = time.perf_counter()
    try:
        result = subprocess.run(
            ["pandoc", "-f", "latex", "-t", "typst", "--wrap=none"],
            input=input_text,
            capture_output=True,
            text=True,
            timeout=30
        )
        elapsed = (time.perf_counter() - start) * 1000
        if result.returncode != 0:
            return f"Error: {result.stderr}", elapsed
        return result.stdout.strip(), elapsed
    except FileNotFoundError:
        elapsed = (time.perf_counter() - start) * 1000
        return "Error: Pandoc not installed", elapsed
    except Exception as e:
        elapsed = (time.perf_counter() - start) * 1000
        return f"Error: {e}", elapsed


def compare_single(latex: str, description: str = ""):
    """Compare single expression"""
    print(f"\n{'='*60}")
    if description:
        print(f"Test: {description}")
    print(f"{'─'*60}")
    print(f"LaTeX:  {latex}")

    tylax_out, tylax_time = run_tylax(f"${latex}$")
    pandoc_out, pandoc_time = run_pandoc(f"${latex}$")

    print(f"Tylax: {tylax_out} ({tylax_time:.2f}ms)")
    print(f"Pandoc: {pandoc_out} ({pandoc_time:.2f}ms)")

    if tylax_out == pandoc_out:
        print("Result: ✓ MATCH")
    else:
        print("Result: ≠ DIFFER")

    return tylax_out, pandoc_out, tylax_time, pandoc_time


def run_all_tests():
    """Run all test cases"""
    print("╔" + "═"*58 + "╗")
    print("║" + "Tylax vs Pandoc: LaTeX → Typst Comparison".center(58) + "║")
    print("╚" + "═"*58 + "╝")

    results = {
        "tylax_success": 0,
        "pandoc_success": 0,
        "matches": 0,
        "tylax_time": 0,
        "pandoc_time": 0,
        "total": len(MATH_TEST_CASES)
    }

    for latex, desc in MATH_TEST_CASES:
        tylax_out, pandoc_out, tylax_time, pandoc_time = compare_single(latex, desc)

        if not tylax_out.startswith("Error"):
            results["tylax_success"] += 1
        if not pandoc_out.startswith("Error"):
            results["pandoc_success"] += 1
        if tylax_out == pandoc_out:
            results["matches"] += 1

        results["tylax_time"] += tylax_time
        results["pandoc_time"] += pandoc_time

    # Summary
    print("\n")
    print("╔" + "═"*58 + "╗")
    print("║" + "Summary".center(58) + "║")
    print("╚" + "═"*58 + "╝")
    print(f"Total tests:      {results['total']}")
    print(f"Tylax success:   {results['tylax_success']}/{results['total']}")
    print(f"Pandoc success:   {results['pandoc_success']}/{results['total']}")
    print(f"Exact matches:    {results['matches']}/{results['total']}")
    print()
    print("Performance:")
    print(f"  Tylax total: {results['tylax_time']:.2f}ms")
    print(f"  Pandoc total: {results['pandoc_time']:.2f}ms")
    if results['tylax_time'] > 0:
        print(f"  Speedup:      {results['pandoc_time']/results['tylax_time']:.1f}x")


def run_document_test():
    """Test document conversion"""
    print("╔" + "═"*58 + "╗")
    print("║" + "Document Conversion Comparison".center(58) + "║")
    print("╚" + "═"*58 + "╝")

    print("\nInput LaTeX:")
    print("─"*60)
    print(DOCUMENT_TEST)

    tylax_out, tylax_time = run_tylax(DOCUMENT_TEST, mode="document")
    pandoc_out, pandoc_time = run_pandoc(DOCUMENT_TEST)

    print("\n" + "="*60)
    print(f"Tylax Output ({tylax_time:.2f}ms):")
    print("─"*60)
    print(tylax_out)

    print("\n" + "="*60)
    print(f"Pandoc Output ({pandoc_time:.2f}ms):")
    print("─"*60)
    print(pandoc_out)

    if tylax_time > 0:
        print(f"\nPerformance: Tylax is {pandoc_time/tylax_time:.1f}x faster")


def run_benchmark(iterations: int = 100):
    """Run performance benchmark"""
    print("╔" + "═"*58 + "╗")
    print("║" + "Performance Benchmark".center(58) + "║")
    print("╚" + "═"*58 + "╝")

    test_expr = r"\frac{\partial^2 u}{\partial x^2} + \frac{\partial^2 u}{\partial y^2} = 0"

    print(f"\nTest expression: {test_expr}")
    print(f"Iterations: {iterations}")
    print()

    # Tylax benchmark
    tylax_times = []
    for _ in range(iterations):
        _, t = run_tylax(f"${test_expr}$")
        tylax_times.append(t)

    # Pandoc benchmark
    pandoc_times = []
    for _ in range(iterations):
        _, t = run_pandoc(f"${test_expr}$")
        pandoc_times.append(t)

    tylax_avg = sum(tylax_times) / len(tylax_times)
    pandoc_avg = sum(pandoc_times) / len(pandoc_times)

    print(f"Tylax: avg={tylax_avg:.2f}ms, min={min(tylax_times):.2f}ms, max={max(tylax_times):.2f}ms")
    print(f"Pandoc: avg={pandoc_avg:.2f}ms, min={min(pandoc_times):.2f}ms, max={max(pandoc_times):.2f}ms")
    print(f"\nSpeedup: {pandoc_avg/tylax_avg:.1f}x")


def main():
    parser = argparse.ArgumentParser(description="Compare Tylax with Pandoc")
    parser.add_argument("--math", "-m", help="Test single math expression")
    parser.add_argument("--file", "-f", help="Test file")
    parser.add_argument("--document", "-d", action="store_true", help="Test document conversion")
    parser.add_argument("--benchmark", "-b", action="store_true", help="Run benchmark")
    parser.add_argument("--iterations", "-n", type=int, default=10, help="Benchmark iterations")

    args = parser.parse_args()

    if args.math:
        compare_single(args.math)
    elif args.file:
        with open(args.file) as f:
            content = f.read()
        tylax_out, tylax_time = run_tylax(content, mode="document")
        pandoc_out, pandoc_time = run_pandoc(content)
        print(f"Tylax ({tylax_time:.2f}ms):\n{tylax_out}\n")
        print(f"Pandoc ({pandoc_time:.2f}ms):\n{pandoc_out}")
    elif args.document:
        run_document_test()
    elif args.benchmark:
        run_benchmark(args.iterations)
    else:
        run_all_tests()


if __name__ == "__main__":
    main()
