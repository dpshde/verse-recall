from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

import numpy as np


SCRIPT = Path(__file__).parents[1] / "scripts" / "build_semantic_vectors.py"
SPEC = importlib.util.spec_from_file_location("build_semantic_vectors", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
semantic = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = semantic
SPEC.loader.exec_module(semantic)


class BuildSemanticVectorsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def test_projection_is_deterministic_and_normalized(self) -> None:
        vectors = np.asarray(
            [
                [1.0, 0.2, -0.1, 0.4],
                [0.2, 1.0, 0.3, -0.5],
                [-0.4, 0.3, 1.0, 0.1],
                [0.6, -0.2, 0.4, 1.0],
                [-0.5, 0.7, -0.3, 0.2],
            ],
            dtype=np.float32,
        )
        normalized = semantic.normalize_vectors(vectors)
        first = semantic.fit_pca_projection(normalized, 2)
        second = semantic.fit_pca_projection(normalized, 2)
        np.testing.assert_array_equal(first[0], second[0])
        np.testing.assert_array_equal(first[1], second[1])

        one = self.root / "one.projection"
        two = self.root / "two.projection"
        semantic.save_projection(one, *first)
        semantic.save_projection(two, *second)
        self.assertEqual(one.read_bytes(), two.read_bytes())
        projected = semantic.apply_projection(vectors, semantic.load_projection(one))
        self.assertEqual(projected.shape, (5, 2))
        np.testing.assert_allclose(
            np.linalg.norm(projected, axis=1), np.ones(5), atol=1e-6
        )

    def test_projection_rejects_wrong_input_dimension(self) -> None:
        mean = np.zeros(4, dtype=np.float32)
        components = np.eye(4, 2, dtype=np.float32)
        with self.assertRaisesRegex(ValueError, "requires 4D input"):
            semantic.apply_projection(
                np.ones((3, 5), dtype=np.float32), (mean, components)
            )

    def test_query_adapter_round_trip_is_asymmetric_and_normalized(self) -> None:
        matrix = np.asarray(
            [[0.0, 2.0], [1.0, 0.0], [0.0, -1.0]], dtype=np.float32
        )
        adapter = self.root / "domain.query-adapter"
        semantic.save_query_adapter(adapter, matrix)
        np.testing.assert_array_equal(
            semantic.load_query_adapter(adapter), matrix
        )

        vectors = np.asarray([[2.0, 1.0, 0.0], [0.0, 1.0, 1.0]], dtype=np.float32)
        adapted = semantic.apply_query_adapter(
            vectors, semantic.load_query_adapter(adapter)
        )
        self.assertEqual(adapted.shape, (2, 2))
        np.testing.assert_allclose(
            np.linalg.norm(adapted, axis=1), np.ones(2), atol=1e-6
        )
        # The source vectors are never modified in place; corpus generation
        # has no query-adapter option and therefore stays in document space.
        np.testing.assert_array_equal(
            vectors,
            np.asarray([[2.0, 1.0, 0.0], [0.0, 1.0, 1.0]], dtype=np.float32),
        )

        with self.assertRaisesRegex(ValueError, "requires 3D input"):
            semantic.apply_query_adapter(np.ones((1, 4), dtype=np.float32), matrix)

    def test_query_adapter_is_content_addressed_in_space_contract(self) -> None:
        model = self.root / "model.onnx"
        tokenizer = self.root / "tokenizer.json"
        adapter = self.root / "domain.query-adapter"
        model.write_bytes(b"graph")
        tokenizer.write_bytes(b"tokenizer")
        semantic.save_query_adapter(adapter, np.eye(2, dtype=np.float32))

        plain = semantic.embedding_space_contract(
            model=model,
            tokenizer=tokenizer,
            pooling="cls",
            max_length=128,
            query_prefix="query: ",
            corpus_prefix="",
            projection=None,
        )
        adapted = semantic.embedding_space_contract(
            model=model,
            tokenizer=tokenizer,
            pooling="cls",
            max_length=128,
            query_prefix="query: ",
            corpus_prefix="",
            projection=None,
            query_adapter=adapter,
        )
        self.assertNotIn("query_adapter_sha256", plain)
        self.assertEqual(
            adapted["query_adapter_application"], "post-encoder-query-only-v1"
        )
        self.assertNotEqual(
            semantic.contract_space_id(plain), semantic.contract_space_id(adapted)
        )

        semantic.save_query_adapter(adapter, np.asarray([[0.0, 1.0], [1.0, 0.0]]))
        changed = semantic.embedding_space_contract(
            model=model,
            tokenizer=tokenizer,
            pooling="cls",
            max_length=128,
            query_prefix="query: ",
            corpus_prefix="",
            projection=None,
            query_adapter=adapter,
        )
        self.assertNotEqual(
            semantic.contract_space_id(adapted), semantic.contract_space_id(changed)
        )

    def test_matryoshka_truncation_is_normalized_and_has_requested_shape(self) -> None:
        vectors = np.asarray(
            [
                [3.0, 4.0, 12.0, 0.0],
                [5.0, -12.0, 0.0, 84.0],
            ],
            dtype=np.float32,
        )
        truncated = semantic.apply_projection(vectors, None, truncate_dims=2)

        self.assertEqual(truncated.shape, (2, 2))
        np.testing.assert_allclose(
            np.linalg.norm(truncated, axis=1), np.ones(2), atol=1e-6
        )
        expected = semantic.normalize_vectors(
            semantic.normalize_vectors(vectors)[:, :2]
        )
        np.testing.assert_allclose(truncated, expected, atol=1e-7)

    def test_matryoshka_truncation_rejects_invalid_dimensions_and_projection(self) -> None:
        vectors = np.ones((2, 4), dtype=np.float32)
        for invalid in (0, -1, 5):
            with self.subTest(truncate_dims=invalid):
                with self.assertRaisesRegex(ValueError, "truncate dimensions"):
                    semantic.apply_projection(vectors, None, truncate_dims=invalid)

        projection = (np.zeros(4, dtype=np.float32), np.eye(4, 2, dtype=np.float32))
        with self.assertRaisesRegex(ValueError, "cannot be combined"):
            semantic.apply_projection(vectors, projection, truncate_dims=2)

    def test_pre_truncate_layer_norm_matches_reference_and_is_addressed(self) -> None:
        vectors = np.asarray(
            [[1.0, 2.0, 8.0, -3.0], [9.0, 2.0, 1.0, -7.0]],
            dtype=np.float32,
        )
        transformed = semantic.apply_projection(
            vectors,
            None,
            truncate_dims=2,
            pre_truncate_layer_norm=True,
        )
        mean = vectors.mean(axis=1, keepdims=True)
        variance = ((vectors - mean) ** 2).mean(axis=1, keepdims=True)
        expected = semantic.normalize_vectors(
            ((vectors - mean) / np.sqrt(variance + 1e-5))[:, :2]
        )
        np.testing.assert_allclose(transformed, expected, atol=1e-6)

        model = self.root / "model.onnx"
        tokenizer = self.root / "tokenizer.json"
        model.write_bytes(b"graph")
        tokenizer.write_bytes(b"tokenizer")
        plain = semantic.embedding_space_contract(
            model=model,
            tokenizer=tokenizer,
            pooling="mean",
            max_length=128,
            query_prefix="search_query: ",
            corpus_prefix="search_document: ",
            projection=None,
            truncate_dims=2,
        )
        layered = semantic.embedding_space_contract(
            model=model,
            tokenizer=tokenizer,
            pooling="mean",
            max_length=128,
            query_prefix="search_query: ",
            corpus_prefix="search_document: ",
            projection=None,
            truncate_dims=2,
            pre_truncate_layer_norm=True,
        )
        self.assertEqual(
            layered["pre_truncate_layer_norm"],
            "per-row-epsilon-1e-5-v1",
        )
        self.assertNotEqual(
            semantic.contract_space_id(plain), semantic.contract_space_id(layered)
        )

    def test_space_id_covers_external_weights_tokenizer_and_projection(self) -> None:
        model = self.root / "model.onnx"
        external = self.root / "model.onnx_data"
        tokenizer = self.root / "tokenizer.json"
        projection = self.root / "projection.bin"
        model.write_bytes(b"graph")
        external.write_bytes(b"weights-v1")
        tokenizer.write_bytes(b"tokenizer")
        projection.write_bytes(b"projection")

        def identity() -> str:
            contract = semantic.embedding_space_contract(
                model=model,
                tokenizer=tokenizer,
                pooling="cls",
                max_length=128,
                query_prefix="Represent this sentence: ",
                corpus_prefix="",
                projection=projection,
            )
            self.assertEqual(
                [asset["name"] for asset in contract["encoder_assets"]],
                ["model.onnx", "model.onnx_data"],
            )
            return semantic.contract_space_id(contract)

        original = identity()
        self.assertEqual(len(original), 64)
        self.assertEqual(original, identity())
        external.write_bytes(b"weights-v2")
        self.assertNotEqual(original, identity())

    def test_space_id_changes_only_when_truncation_is_enabled(self) -> None:
        model = self.root / "model.onnx"
        tokenizer = self.root / "tokenizer.json"
        model.write_bytes(b"graph")
        tokenizer.write_bytes(b"tokenizer")

        def contract(truncate_dims: int | None = None) -> dict:
            return semantic.embedding_space_contract(
                model=model,
                tokenizer=tokenizer,
                pooling="cls",
                max_length=128,
                query_prefix="Represent this sentence: ",
                corpus_prefix="",
                projection=None,
                truncate_dims=truncate_dims,
            )

        unset = contract()
        explicit_none = contract(None)
        truncated = contract(2)
        self.assertNotIn("truncate_dims", unset)
        self.assertEqual(unset, explicit_none)
        self.assertEqual(
            semantic.contract_space_id(unset),
            "25aded4611a53f78118cacd2aee8285d83167ecdd1ff05303d749211c8989709",
        )
        self.assertEqual(
            semantic.contract_space_id(unset),
            semantic.contract_space_id(explicit_none),
        )
        self.assertEqual(truncated["truncate_dims"], 2)
        self.assertNotEqual(
            semantic.contract_space_id(unset),
            semantic.contract_space_id(truncated),
        )

        with self.assertRaisesRegex(ValueError, "must be positive"):
            contract(0)

    def test_query_segment_planner_is_conservative_and_unicode_safe(self) -> None:
        self.assertEqual(
            semantic.plan_query_segments(
                "Jesus calms the storm; Paul and Silas sing in prison"
            ),
            ["Jesus calms the storm", "Paul and Silas sing in prison"],
        )
        self.assertEqual(
            semantic.plan_query_segments("lost sheep and prodigal son"),
            ["lost sheep", "prodigal son"],
        )
        self.assertEqual(
            semantic.plan_query_segments("Élie prie — Jésus nourrit la foule"),
            ["Élie prie", "Jésus nourrit la foule"],
        )
        self.assertEqual(
            semantic.plan_query_segments(
                "rain nourishes a field because another image describes a fruitful tree"
            ),
            ["rain nourishes a field", "another image describes a fruitful tree"],
        )
        self.assertEqual(
            semantic.plan_query_segments(
                "Jesus calms storms; Paul survives shipwrecks; "
                "Peter walks water; Philip meets travelers"
            ),
            [
                "Jesus calms storms",
                "Paul survives shipwrecks",
                "Peter walks water",
                "Philip meets travelers",
            ],
        )
        self.assertEqual(semantic.plan_query_segments("faith and hope"), [])
        self.assertEqual(
            semantic.plan_query_segments("walk by faith and not by sight"), []
        )
        self.assertEqual(semantic.plan_query_segments("the fall"), [])

    def test_query_segment_planner_matches_shared_rust_conformance_cases(self) -> None:
        cases_path = Path(__file__).with_name("query_segment_planner_cases.json")
        cases = json.loads(cases_path.read_text(encoding="utf-8"))
        for case in cases:
            with self.subTest(query=case["query"]):
                self.assertEqual(
                    semantic.plan_query_segments(case["query"]), case["segments"]
                )

    def test_query_records_add_segments_only_when_opted_in(self) -> None:
        class FakeEncoder:
            def __init__(self) -> None:
                self.texts: list[str] = []

            def encode(self, texts: list[str], batch_size: int) -> np.ndarray:
                self.texts = texts
                self.batch_size = batch_size
                return np.asarray(
                    [[float(index), float(index + 1)] for index in range(len(texts))],
                    dtype=np.float32,
                )

        rows = [
            {"id": "multi", "prompt": "lost sheep and prodigal son"},
            {"id": "single", "prompt": "the fall"},
        ]
        legacy_encoder = FakeEncoder()
        legacy = semantic.encode_query_records(
            rows, legacy_encoder, "query: ", 8, segment_embeddings=False
        )
        self.assertEqual(
            legacy,
            [
                {
                    "id": "multi",
                    "prompt_utf8_sha256": semantic.prompt_utf8_sha256(
                        "lost sheep and prodigal son"
                    ),
                    "embedding": [0.0, 1.0],
                },
                {
                    "id": "single",
                    "prompt_utf8_sha256": semantic.prompt_utf8_sha256("the fall"),
                    "embedding": [1.0, 2.0],
                },
            ],
        )
        self.assertEqual(
            legacy_encoder.texts,
            ["query: lost sheep and prodigal son", "query: the fall"],
        )

        segment_encoder = FakeEncoder()
        enriched = semantic.encode_query_records(
            rows, segment_encoder, "query: ", 8, segment_embeddings=True
        )
        self.assertEqual(
            segment_encoder.texts,
            [
                "query: lost sheep and prodigal son",
                "query: the fall",
                "query: lost sheep",
                "query: prodigal son",
            ],
        )
        self.assertEqual(enriched[0]["planner"], semantic.QUERY_SEGMENT_PLANNER)
        self.assertEqual(
            enriched[0]["segments"],
            [
                {"text": "lost sheep", "embedding": [2.0, 3.0]},
                {"text": "prodigal son", "embedding": [3.0, 4.0]},
            ],
        )
        self.assertNotIn("planner", enriched[1])
        self.assertNotIn("segments", enriched[1])
        self.assertEqual(
            enriched[0]["prompt_utf8_sha256"],
            semantic.prompt_utf8_sha256("lost sheep and prodigal son"),
        )

        adapted = semantic.encode_query_records(
            rows,
            FakeEncoder(),
            "query: ",
            8,
            segment_embeddings=True,
            query_adapter=np.asarray([[0.0, 1.0], [1.0, 0.0]], dtype=np.float32),
        )
        for record in adapted:
            np.testing.assert_allclose(
                np.linalg.norm(record["embedding"]), 1.0, atol=1e-6
            )
        for segment in adapted[0]["segments"]:
            np.testing.assert_allclose(
                np.linalg.norm(segment["embedding"]), 1.0, atol=1e-6
            )

    def test_adapt_query_file_transforms_whole_and_segment_vectors(self) -> None:
        source = self.root / "queries.jsonl"
        output = self.root / "adapted.jsonl"
        adapter = self.root / "adapter.bin"
        source.write_text(
            json.dumps(
                {
                    "id": "multi",
                    "prompt_utf8_sha256": semantic.prompt_utf8_sha256(
                        "a remembered multi-intent prompt"
                    ),
                    "embedding": [1.0, 0.0],
                    "planner": semantic.QUERY_SEGMENT_PLANNER,
                    "segments": [{"text": "part", "embedding": [0.0, 1.0]}],
                }
            )
            + "\n",
            encoding="utf-8",
        )
        semantic.save_query_adapter(
            adapter, np.asarray([[0.0, 1.0], [1.0, 0.0]], dtype=np.float32)
        )
        args = type(
            "Args",
            (),
            {"input": source, "output": output, "query_adapter": adapter},
        )()
        semantic.adapt_query_file(args)
        row = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(
            row["prompt_utf8_sha256"],
            semantic.prompt_utf8_sha256("a remembered multi-intent prompt"),
        )
        np.testing.assert_allclose(row["embedding"], [0.0, 1.0], atol=1e-7)
        np.testing.assert_allclose(
            row["segments"][0]["embedding"], [1.0, 0.0], atol=1e-7
        )

    def test_prompt_binding_hashes_exact_utf8_and_adapt_requires_it(self) -> None:
        prompt = "  Lógos 🌿  "
        self.assertEqual(
            semantic.prompt_utf8_sha256(prompt),
            "1cc71a1e060ea5033a6f23c692126098d10707b974b5eb461e3bce990f17dc26",
        )

        source = self.root / "unbound.jsonl"
        output = self.root / "adapted.jsonl"
        adapter = self.root / "adapter.bin"
        source.write_text(
            json.dumps({"id": "unbound", "embedding": [1.0, 0.0]}) + "\n",
            encoding="utf-8",
        )
        semantic.save_query_adapter(adapter, np.eye(2, dtype=np.float32))
        args = type(
            "Args",
            (),
            {"input": source, "output": output, "query_adapter": adapter},
        )()
        with self.assertRaisesRegex(ValueError, "prompt_utf8_sha256"):
            semantic.adapt_query_file(args)

    def test_query_adapter_matches_shared_double_accumulation_golden(self) -> None:
        fixture_path = (
            Path(__file__).parents[1]
            / "tests"
            / "fixtures"
            / "vrqadp01-golden-v1.json"
        )
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        self.assertEqual(
            fixture["schema"], "verse-recall-vrqadp01-arithmetic-v1"
        )
        matrix = np.asarray(fixture["matrix"], dtype=np.float32).reshape(
            fixture["input_dimensions"], fixture["output_dimensions"]
        )
        output = semantic.apply_query_adapter(
            np.asarray([fixture["embedding"]], dtype=np.float32), matrix
        )[0]
        actual_bits = [f"{value:08x}" for value in output.view(np.uint32)]
        self.assertEqual(actual_bits, fixture["expected_f32_bits"])

    def test_corpus_can_build_windows_without_reencoding_verses(self) -> None:
        corpus_path = self.root / "corpus.jsonl"
        corpus_path.write_text(
            '{"ref":"Genesis 1:1","text":"first verse"}\n'
            '{"ref":"Genesis 1:2","text":"second verse"}\n',
            encoding="utf-8",
        )
        window_output = self.root / "windows.f32"

        class FakeEncoder:
            def __init__(self) -> None:
                self.calls: list[list[str]] = []

            def encode(self, texts: list[str], batch_size: int) -> np.ndarray:
                self.calls.append(texts)
                self.batch_size = batch_size
                return np.ones((len(texts), 2), dtype=np.float32)

        encoder = FakeEncoder()
        args = type(
            "Args",
            (),
            {
                "input": corpus_path,
                "corpus_prefix": "passage: ",
                "batch_size": 4,
                "verse_output": None,
                "window_output": window_output,
            },
        )()
        with mock.patch.object(semantic, "build_encoder", return_value=encoder):
            semantic.corpus(args)

        self.assertEqual(
            encoder.calls,
            [["passage: first verse second verse", "passage: first verse second verse"]],
        )
        self.assertEqual(window_output.stat().st_size, 2 * 2 * 4)

        args.window_output = None
        with self.assertRaisesRegex(ValueError, "verse-output or window-output"):
            semantic.corpus(args)

    def test_windows_never_cross_chapter_or_book_boundaries(self) -> None:
        rows = [
            {"ref": "Genesis 1:31"},
            {"ref": "Genesis 2:1"},
            {"ref": "Genesis 2:2"},
            {"ref": "Exodus 1:1"},
        ]
        self.assertEqual(
            semantic.chapter_bounded_windows(rows, ["g1", "g2a", "g2b", "e1"]),
            ["g1", "g2a g2b", "g2a g2b", "e1"],
        )
        with self.assertRaisesRegex(ValueError, "requires a canonical ref"):
            semantic.chapter_bounded_windows([{"text": "missing"}], ["missing"])


if __name__ == "__main__":
    unittest.main()
