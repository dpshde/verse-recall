from __future__ import annotations

import copy
import unittest

from scripts.embedding_space_contract import contract_space_id, validate_envelope


class EmbeddingSpaceContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = {
            "schema": "verse-recall-embedding-space-v1",
            "encoder_assets": [{"name": "model.onnx", "sha256": "1" * 64}],
            "tokenizer_sha256": "2" * 64,
            "pooling": "cls",
            "max_length": 128,
            "query_prefix": "query: ",
            "corpus_prefix": "",
            "projection_sha256": None,
            "output_normalization": "l2-f32-v1",
            "truncate_dims": 256,
        }

    def envelope(self, contract: dict | None = None) -> dict:
        contract = contract or self.contract
        return {
            "contract": contract,
            "embedding_space_id": contract_space_id(contract),
        }

    def test_accepts_matryoshka_contract(self) -> None:
        contract, space_id = validate_envelope(self.envelope())
        self.assertIs(contract, self.contract)
        self.assertEqual(space_id, contract_space_id(self.contract))

    def test_accepts_declared_pre_truncation_layer_norm(self) -> None:
        contract = copy.deepcopy(self.contract)
        contract["pre_truncate_layer_norm"] = "per-row-epsilon-1e-5-v1"
        validate_envelope(self.envelope(contract))

    def test_rejects_unknown_or_incomplete_runtime_contracts(self) -> None:
        for field, value, message in (
            ("output_normalization", "none", "normalization"),
            ("pooling", "last-token", "pooling"),
            ("max_length", 0, "positive integer"),
            ("query_prefix", 7, "must be a string"),
            ("pre_truncate_layer_norm", "mystery", "pre-truncation"),
        ):
            with self.subTest(field=field):
                contract = copy.deepcopy(self.contract)
                contract[field] = value
                with self.assertRaisesRegex(ValueError, message):
                    validate_envelope(self.envelope(contract))

    def test_rejects_bare_contract_and_space_id_mismatch(self) -> None:
        with self.assertRaisesRegex(ValueError, "exactly contract"):
            validate_envelope(self.contract)
        envelope = self.envelope()
        envelope["embedding_space_id"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "does not match"):
            validate_envelope(envelope)


if __name__ == "__main__":
    unittest.main()
