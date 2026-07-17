from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

import numpy as np


SCRIPT = Path(__file__).parents[1] / "scripts" / "source_query_adapter.py"
SPEC = importlib.util.spec_from_file_location("source_query_adapter", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
adapter = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = adapter
SPEC.loader.exec_module(adapter)


def parameters() -> adapter.AdapterParameters:
    return adapter.AdapterParameters(
        down_weight=np.asarray(
            [[0.2, -0.1], [0.3, 0.4], [-0.2, 0.1]], dtype=np.float32
        ),
        down_bias=np.asarray([0.01, -0.02], dtype=np.float32),
        up_weight=np.asarray(
            [[0.1, -0.2, 0.3], [-0.3, 0.1, 0.2]], dtype=np.float32
        ),
        up_bias=np.asarray([0.01, 0.0, -0.01], dtype=np.float32),
        residual_scale=0.5,
    )


class SourceQueryAdapterTest(unittest.TestCase):
    def test_round_trip_is_content_addressed_and_byte_deterministic(self) -> None:
        first = adapter.serialize_adapter(parameters())
        second = adapter.serialize_adapter(parameters())
        self.assertEqual(first, second)
        with tempfile.TemporaryDirectory() as directory:
            path = adapter.save_content_addressed(Path(directory), parameters())
            self.assertIn(adapter.content_sha256(first), path.name)
            loaded = adapter.load_content_addressed(path)
            np.testing.assert_array_equal(
                loaded.down_weight, parameters().down_weight
            )
            self.assertEqual(path, adapter.save_content_addressed(Path(directory), loaded))

    def test_numpy_apply_normalizes_and_supports_single_vectors(self) -> None:
        vectors = np.asarray([[2.0, 1.0, 0.5], [0.5, -1.0, 2.0]], dtype=np.float32)
        matrix = adapter.apply_adapter(vectors, parameters(), batch_size=1)
        single = adapter.apply_adapter(vectors[0], parameters())
        np.testing.assert_allclose(single, matrix[0], atol=1e-7)
        np.testing.assert_allclose(
            np.linalg.norm(matrix, axis=1), np.ones(2), atol=1e-6
        )

    def test_loader_rejects_filename_or_content_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = adapter.save_content_addressed(Path(directory), parameters())
            malformed = Path(directory) / "mutable-name.vrqa"
            malformed.write_bytes(path.read_bytes())
            with self.assertRaisesRegex(ValueError, "not content-addressed"):
                adapter.load_content_addressed(malformed)

            data = bytearray(path.read_bytes())
            data[-1] ^= 1
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                adapter.load_content_addressed(path)

    def test_parameter_and_input_validation_fail_closed(self) -> None:
        invalid = adapter.AdapterParameters(
            down_weight=np.zeros((3, 2), dtype=np.float32),
            down_bias=np.zeros(3, dtype=np.float32),
            up_weight=np.zeros((2, 3), dtype=np.float32),
            up_bias=np.zeros(3, dtype=np.float32),
        )
        with self.assertRaisesRegex(ValueError, "down_bias shape"):
            adapter.serialize_adapter(invalid)
        with self.assertRaisesRegex(ValueError, "zero-norm"):
            adapter.apply_adapter(np.zeros(3, dtype=np.float32), parameters())


if __name__ == "__main__":
    unittest.main()
