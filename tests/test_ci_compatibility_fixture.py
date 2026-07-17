from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import unittest


ROOT = Path(__file__).parents[1]
MANIFEST_PATH = ROOT / "eval" / "ci-compatibility-arctic-m256-query.manifest.json"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


class CiCompatibilityFixtureTest(unittest.TestCase):
    def test_fixture_is_content_addressed_and_prompt_bound(self) -> None:
        manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
        self.assertEqual(
            {
                "schema",
                "purpose",
                "prompts",
                "query_embeddings",
                "embedding_space_id",
                "generation",
            },
            set(manifest),
        )
        self.assertEqual("verse-recall-query-fixture-v1", manifest["schema"])
        self.assertIn("not semantic release evidence", manifest["purpose"])

        prompt_path = ROOT / manifest["prompts"]["path"]
        query_path = ROOT / manifest["query_embeddings"]["path"]
        for identity, path in (
            (manifest["prompts"], prompt_path),
            (manifest["query_embeddings"], query_path),
        ):
            self.assertEqual(identity["bytes"], path.stat().st_size)
            self.assertEqual(identity["sha256"], sha256(path))

        contract = json.loads(
            (ROOT / "data" / "semantic-space-arctic-m-v1.5-m256.json").read_text(
                encoding="utf-8"
            )
        )
        generation = manifest["generation"]
        self.assertEqual(contract["embedding_space_id"], manifest["embedding_space_id"])
        self.assertEqual(
            contract["contract"]["encoder_assets"][0]["sha256"],
            generation["model_sha256"],
        )
        self.assertEqual(
            contract["contract"]["tokenizer_sha256"],
            generation["tokenizer_sha256"],
        )
        for field in ("pooling", "max_length", "truncate_dims", "query_prefix"):
            self.assertEqual(contract["contract"][field], generation[field])
        self.assertTrue(generation["segment_embeddings"])

        prompt_records = read_jsonl(prompt_path)
        query_records = read_jsonl(query_path)
        prompts = {record["id"]: record for record in prompt_records}
        queries = {record["id"]: record for record in query_records}
        self.assertEqual(len(prompts), len(prompt_records))
        self.assertEqual(len(queries), len(query_records))
        self.assertEqual(set(prompts), set(queries))
        for prompt_id, query in queries.items():
            prompt = prompts[prompt_id]["prompt"]
            self.assertEqual(
                hashlib.sha256(prompt.encode("utf-8")).hexdigest(),
                query["prompt_utf8_sha256"],
            )
            self.assertEqual(256, len(query["embedding"]))
            self.assertTrue(all(math.isfinite(value) for value in query["embedding"]))
            if query.get("segments"):
                self.assertEqual("verse-recall-clause-v2", query.get("planner"))
                for segment in query["segments"]:
                    self.assertTrue(segment["text"].strip())
                    self.assertEqual(256, len(segment["embedding"]))
                    self.assertTrue(
                        all(math.isfinite(value) for value in segment["embedding"])
                    )


if __name__ == "__main__":
    unittest.main()
