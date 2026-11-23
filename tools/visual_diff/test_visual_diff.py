import importlib.util
import os
import shutil
import struct
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("visual_diff.py")
SPEC = importlib.util.spec_from_file_location("visual_diff", MODULE_PATH)
visual_diff = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(visual_diff)


class VisualDiffRegressionTests(unittest.TestCase):
    def test_relative_cargo_target_dir_is_resolved_from_repository(self):
        with mock.patch.dict(os.environ, {"CARGO_TARGET_DIR": "target-cache"}):
            path = visual_diff.driver_path()

        self.assertTrue(os.path.isabs(path))
        self.assertEqual(
            os.path.normcase(path),
            os.path.normcase(
                os.path.join(
                    visual_diff.REPO,
                    "target-cache",
                    "release",
                    "examples",
                    "l2t_batch.exe" if os.name == "nt" else "l2t_batch",
                )
            ),
        )

    def test_missing_input_does_not_remove_existing_output(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "results"
            marker = out / "mathjax" / "preserve.txt"
            marker.parent.mkdir(parents=True)
            marker.write_text("existing output", encoding="utf-8")
            args = Namespace(
                no_build=True,
                out=str(out),
                input=str(Path(directory) / "missing.json"),
                sample=0,
                seed=1,
            )

            with mock.patch.object(visual_diff, "require_tools"):
                with self.assertRaises(FileNotFoundError):
                    visual_diff.cmd_run(args)

            self.assertEqual(marker.read_text(encoding="utf-8"), "existing output")

    @unittest.skipUnless(shutil.which("typst"), "Typst CLI is required")
    def test_inline_equation_is_rendered_in_display_style(self):
        with tempfile.TemporaryDirectory() as directory:
            display_png, display_error = visual_diff.compile_batch_once(
                directory, "display", "$ a/b $"
            )
            inline_png, inline_error = visual_diff.compile_batch_once(
                directory, "inline", "$a/b$"
            )

            self.assertIsNone(display_error)
            self.assertIsNone(inline_error)
            self.assertEqual(png_dimensions(display_png), png_dimensions(inline_png))

    @unittest.skipUnless(shutil.which("typst"), "Typst CLI is required")
    def test_label_after_equation_is_preserved_in_display_style(self):
        with tempfile.TemporaryDirectory() as directory:
            labeled_png, error = visual_diff.compile_batch_once(
                directory, "labeled", "$ a=b $ <eq-a>"
            )
            display_png, display_error = visual_diff.compile_batch_once(
                directory, "display", "$ a=b $"
            )

            self.assertIsNone(error)
            self.assertIsNone(display_error)
            self.assertEqual(png_dimensions(labeled_png), png_dimensions(display_png))


def png_dimensions(path):
    with open(path, "rb") as image:
        header = image.read(24)
    return struct.unpack(">II", header[16:24])


if __name__ == "__main__":
    unittest.main()
