import unittest

import numpy as np

from scripts.build_study_pca_alignment import fit_pca_components, project_in_batches


class BuildStudyPcaAlignmentTests(unittest.TestCase):
    def test_streamed_pca_is_deterministic_and_projects_to_requested_dims(self) -> None:
        vectors = np.asarray(
            [
                [1.0, 0.0, 0.0],
                [0.8, 0.2, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.8, 0.2],
                [0.0, 0.0, 1.0],
                [0.2, 0.0, 0.8],
            ],
            dtype=np.float32,
        )
        indices = np.arange(len(vectors), dtype=np.int64)
        first = fit_pca_components(vectors, indices, 2, batch_size=2)
        second = fit_pca_components(vectors, indices, 2, batch_size=2)
        np.testing.assert_array_equal(first[0], second[0])
        np.testing.assert_array_equal(first[1], second[1])
        projected = project_in_batches(vectors, first[0], first[1], batch_size=2)
        self.assertEqual(projected.shape, (6, 2))
        np.testing.assert_allclose(np.linalg.norm(projected, axis=1), 1.0, atol=1e-6)

    def test_pca_rejects_invalid_output_dimensions(self) -> None:
        vectors = np.eye(3, dtype=np.float32)
        indices = np.arange(3, dtype=np.int64)
        with self.assertRaisesRegex(ValueError, "dimensions"):
            fit_pca_components(vectors, indices, 4)


if __name__ == "__main__":
    unittest.main()
