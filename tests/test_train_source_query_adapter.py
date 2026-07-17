from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

import numpy as np
import torch


SCRIPTS = Path(__file__).parents[1] / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))
SCRIPT = SCRIPTS / "train_source_query_adapter.py"
SPEC = importlib.util.spec_from_file_location("train_source_query_adapter", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
trainer = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = trainer
SPEC.loader.exec_module(trainer)

runtime = sys.modules["source_query_adapter"]


class TrainSourceQueryAdapterTest(unittest.TestCase):
    def test_evaluation_tree_is_rejected_before_file_access(self) -> None:
        forbidden = trainer.PROJECT_ROOT / "eval" / "does-not-need-to-exist.jsonl"
        with self.assertRaisesRegex(ValueError, "evaluation paths are forbidden"):
            trainer.reject_evaluation_path(forbidden, must_exist=True)

    def test_group_batches_keep_every_exact_source_range_together(self) -> None:
        indices = np.arange(9, dtype=np.int64)
        groups = np.asarray([0, 0, 0, 1, 2, 2, 3, 4, 4], dtype=np.int64)
        batches = list(
            trainer.training_group_batches(
                indices, groups, batch_size=4, seed=7, epoch=1
            )
        )
        flattened = np.concatenate(batches)
        self.assertEqual(set(flattened.tolist()), set(indices.tolist()))
        for group in set(groups.tolist()):
            containing = [
                batch for batch in batches if np.any(groups[batch] == group)
            ]
            self.assertEqual(len(containing), 1)
            self.assertEqual(
                int(np.count_nonzero(groups[containing[0]] == group)),
                int(np.count_nonzero(groups == group)),
            )

    def test_multi_positive_targets_are_not_hard_negatives(self) -> None:
        source = torch.tensor(
            [[1.0, 0.0], [1.0, 0.0], [0.0, 1.0]], dtype=torch.float32
        )
        targets = torch.tensor(
            [[0.9, 0.1], [1.0, 0.0], [0.0, 1.0]], dtype=torch.float32
        )
        groups = torch.tensor([0, 0, 1], dtype=torch.int64)
        config = trainer.LossConfig(0.1, 1, 0.05, 1.0, 0.1, 0.0, 1.0, 1.0)
        loss, parts = trainer.multi_positive_loss(
            source,
            source,
            targets,
            groups,
            torch.tensor([2, 2, 1], dtype=torch.int64),
            config,
        )
        self.assertTrue(bool(torch.isfinite(loss)))
        self.assertEqual(parts["hard_negative"], 0.0)

    def test_torch_and_portable_numpy_application_match(self) -> None:
        model = trainer.ResidualQueryAdapter(4, 3, 0.75, seed=11)
        with torch.no_grad():
            model.up.weight.copy_(
                torch.arange(12, dtype=torch.float32).reshape(4, 3) / 100.0
            )
            model.up.bias.copy_(torch.tensor([0.01, -0.02, 0.03, -0.01]))
        vectors = np.asarray(
            [[0.2, 0.5, -0.4, 0.1], [0.7, -0.3, 0.2, 0.4]],
            dtype=np.float32,
        )
        with torch.no_grad():
            expected = model(torch.from_numpy(vectors)).numpy()
        actual = runtime.apply_adapter(vectors, model.numpy_parameters())
        np.testing.assert_allclose(actual, expected, atol=2e-7)

    def test_end_to_end_toy_manifest_and_full_rerun_are_deterministic(self) -> None:
        rng = np.random.default_rng(19)
        dimensions = 4
        rows = []
        source_rows = []
        target_rows = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for book_number in range(10):
                book = f"Book{book_number:02d}"
                source = rng.normal(size=dimensions).astype(np.float32)
                target = source + np.asarray(
                    [source[1] ** 2, -source[0] * source[2], 0.1, -0.1],
                    dtype=np.float32,
                )
                rows.append(
                    {
                        "ref": f"{book} 1:1",
                        "text": f"source-authored note {book_number}",
                        "view": "study-note",
                    }
                )
                source_rows.append(source)
                target_rows.append(target)
                rows.append(
                    {
                        "ref": f"{book} 1:2",
                        "text": f"canonical fallback {book_number}",
                        "view": "canonical-fallback",
                    }
                )
                fallback = rng.normal(size=dimensions).astype(np.float32)
                source_rows.append(fallback)
                target_rows.append(fallback)

            metadata = root / "metadata.jsonl"
            metadata.write_text(
                "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8"
            )
            source_path = root / "source.f32"
            target_path = root / "target.f32"
            np.asarray(source_rows, dtype="<f4").tofile(source_path)
            np.asarray(target_rows, dtype="<f4").tofile(target_path)
            output = root / "output"
            args = trainer.parse_args(
                [
                    "--metadata",
                    str(metadata),
                    "--source-vectors",
                    str(source_path),
                    "--target-vectors",
                    str(target_path),
                    "--dimensions",
                    str(dimensions),
                    "--output-dir",
                    str(output),
                    "--allow-custom-source-inputs",
                    "--holdout-stride",
                    "3",
                    "--selection-limit",
                    "4",
                    "--epochs",
                    "1",
                    "--train-batch-size",
                    "8",
                    "--validation-batch-size",
                    "2",
                    "--candidate",
                    "toy:3:0.01",
                ]
            )
            report = trainer.run(args)
            self.assertEqual(report["schema"], trainer.MANIFEST_SCHEMA)
            self.assertTrue(
                report["reproducibility"]["full_training_rerun_byte_identical"]
            )
            self.assertIn(
                "macro_average_across_heldout_books", report["source_validation"]
            )
            artifact = output / report["artifact"]["filename"]
            runtime.load_content_addressed(artifact)
            manifest = Path(report["manifest_path"])
            self.assertTrue(manifest.is_file())
            self.assertTrue(report["supervision"]["custom_source_inputs_enabled"])


if __name__ == "__main__":
    unittest.main()
