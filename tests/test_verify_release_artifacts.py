import copy
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from scripts.embedding_space_contract import contract_space_id
from scripts.verify_release_artifacts import (
    BASE_FILENAME,
    BASE_ROLE,
    CONTRACT_FILENAME,
    CONTRACT_ROLE,
    DENSE_FILENAME,
    DENSE_ROLE,
    EXPAND_FILENAME,
    EXPAND_PROVENANCE_FILENAME,
    EXPAND_PROVENANCE_ROLE,
    EXPAND_ROLE,
    OVERLAY_FILENAME,
    OVERLAY_ROLE,
    SCHEMA,
    verify,
)


VERSE_COUNT = 31_086


def align_up(value: int, alignment: int = 64) -> int:
    return (value + alignment - 1) & ~(alignment - 1)


def make_pack(sections: list[tuple[str, bytes]]) -> bytes:
    header = (
        b"VRCALL01"
        + (1).to_bytes(4, "little")
        + b"\0" * 4
        + len(sections).to_bytes(4, "little")
        + b"\0" * 44
    )
    cursor = align_up(64 + len(sections) * 32)
    entries = bytearray()
    offsets: list[int] = []
    for tag, body in sections:
        offsets.append(cursor)
        entries.extend(tag.encode().ljust(8, b"\0"))
        entries.extend(cursor.to_bytes(8, "little"))
        entries.extend(len(body).to_bytes(8, "little"))
        entries.extend(b"\0" * 8)
        cursor = align_up(cursor + len(body))
    output = bytearray(header + entries)
    output.extend(b"\0" * (align_up(len(output)) - len(output)))
    for (_, body), offset in zip(sections, offsets):
        assert len(output) == offset
        output.extend(body)
        output.extend(b"\0" * (align_up(len(output)) - len(output)))
    return bytes(output)


def make_meta(translation: str = "BSB") -> bytes:
    output = bytearray(
        (1).to_bytes(4, "little")
        + VERSE_COUNT.to_bytes(4, "little")
        + len(translation).to_bytes(2, "little")
        + translation.encode()
    )
    output.extend(b"\0" * ((4 - len(output) % 4) % 4))
    output.extend(b"\0" * 16)
    return bytes(output)


@lru_cache(maxsize=1)
def make_text() -> bytes:
    records = bytearray()
    blob = bytearray()
    for index in range(VERSE_COUNT):
        reference = f"R{index}".encode()
        reference_offset = len(blob)
        blob.extend(reference)
        text_offset = len(blob)
        blob.extend(b"x")
        records.extend(reference_offset.to_bytes(4, "little"))
        records.extend(len(reference).to_bytes(4, "little"))
        records.extend(text_offset.to_bytes(4, "little"))
        records.extend((1).to_bytes(4, "little"))
    return (
        VERSE_COUNT.to_bytes(4, "little")
        + bytes(records)
        + len(blob).to_bytes(4, "little")
        + bytes(blob)
    )


def make_lex() -> bytes:
    return (
        b"LEX1"
        + (1).to_bytes(2, "little")
        + (2).to_bytes(2, "little")
        + VERSE_COUNT.to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (3).to_bytes(4, "little")
        + b"\0" * 4
        + (0).to_bytes(4, "little")
        + (3).to_bytes(2, "little")
        + b"\0" * 2
        + (1).to_bytes(4, "little")
        + b"god"
    )


def make_post() -> bytes:
    document_lengths = b"\x01\0" * VERSE_COUNT
    records_start = (32 + len(document_lengths) + 3) & ~3
    padding = b"\0" * (records_start - 32 - len(document_lengths))
    record = (
        (0).to_bytes(4, "little")
        + (2).to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + b"\0" * 4
    )
    return (
        b"PST1"
        + (1).to_bytes(2, "little")
        + b"\0" * 2
        + VERSE_COUNT.to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (2).to_bytes(4, "little")
        + b"\0" * 8
        + document_lengths
        + padding
        + record
        + b"\0\x01"
    )


@lru_cache(maxsize=1)
def make_base_pack() -> bytes:
    return make_pack(
        [
            ("META", make_meta()),
            ("TEXT", make_text()),
            ("LEX", make_lex()),
            ("POST", make_post()),
        ]
    )


def expansion_provenance() -> bytes:
    payload = {
        "schema_version": 1,
        "kind": "manifest",
        "canonical_translation": "BSB",
        "canonical_verse_count": VERSE_COUNT,
        "provenance": {
            "builder": "unit-test-builder",
            "normalizer_version": 2,
            "canonical_sha256": "1" * 64,
            "embeddings_sha256": "2" * 64,
            "vocabulary_sha256": "3" * 64,
            "embedding_rows": 2,
            "embedding_dimensions": 3,
            "source_count": 1,
            "edge_count": 1,
            "parameters": {
                "top_k": 1,
                "similarity_floor": 0.5,
                "batch_size": 2,
                "weight_gamma": 4.0,
                "weight_calibration": "cosine-clamp-power-v1",
            },
        },
    }
    return (json.dumps(payload, separators=(",", ":")) + "\n").encode()


def make_expand(provenance: bytes) -> bytes:
    dictionary = b"alphabeta"
    body = (
        b"VREXP001"
        + (2).to_bytes(2, "little")
        + (2).to_bytes(2, "little")
        + VERSE_COUNT.to_bytes(4, "little")
        + (2).to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (3).to_bytes(2, "little")
        + b"\0" * 2
        + len(dictionary).to_bytes(4, "little")
        + b"\0" * 4
        + hashlib.sha256(provenance).digest()
        + b"BSB"
        + (0).to_bytes(4, "little")
        + (5).to_bytes(2, "little")
        + b"\0" * 2
        + (5).to_bytes(4, "little")
        + (4).to_bytes(2, "little")
        + b"\0" * 2
        + (0).to_bytes(4, "little")
        + (0).to_bytes(4, "little")
        + (1).to_bytes(2, "little")
        + b"\0" * 6
        + (1).to_bytes(4, "little")
        + (65_535).to_bytes(2, "little")
        + b"\0" * 2
        + dictionary
    )
    return make_pack([("META", make_meta()), ("EXPAND", body)])


def encode_string(value: str) -> bytes:
    encoded = value.encode()
    return len(encoded).to_bytes(4, "little") + encoded


def make_overlay() -> bytes:
    body = (
        b"BSB"
        + encode_string("event")
        + encode_string("creation")
        + encode_string("Creation")
        + encode_string("test-source")
        + encode_string("Genesis 1:1")
        + (0).to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + encode_string("Genesis 1:1")
        + encode_string("Genesis 1:2")
    )
    phrase = (
        b"VROVL001"
        + (1).to_bytes(2, "little")
        + b"\0" * 2
        + VERSE_COUNT.to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (0).to_bytes(4, "little")
        + (0).to_bytes(4, "little")
        + (1).to_bytes(4, "little")
        + (3).to_bytes(2, "little")
        + b"\0" * 2
        + len(body).to_bytes(4, "little")
        + body
    )
    return make_pack([("META", make_meta("OVERLAY:BSB")), ("PHRASE", phrase)])


def embedding_contract() -> dict:
    contract = {
        "schema": "verse-recall-embedding-space-v1",
        "encoder_assets": [{"name": "model_quantized.onnx", "sha256": "4" * 64}],
        "tokenizer_sha256": "5" * 64,
        "pooling": "cls",
        "max_length": 128,
        "query_prefix": "Represent this sentence for searching relevant passages: ",
        "corpus_prefix": "",
        "projection_sha256": None,
        "output_normalization": "l2-f32-v1",
        "truncate_dims": 256,
    }
    return {"contract": contract, "embedding_space_id": contract_space_id(contract)}


@lru_cache(maxsize=4)
def make_dense_pack(space_id: str) -> bytes:
    lvec = (
        b"LVC1"
        + (2).to_bytes(2, "little")
        + (256).to_bytes(2, "little")
        + VERSE_COUNT.to_bytes(4, "little")
        + b"\x01\0\0\0"
        + bytes.fromhex(space_id)
        + b"\0" * (VERSE_COUNT * 256)
    )
    return make_pack([("META", make_meta()), ("LVEC", lvec)])


def identity_record(path: Path, role: str, required: bool, **fields: object) -> dict:
    return {
        "filename": path.name,
        "role": role,
        "required": required,
        "bytes": path.stat().st_size,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        **fields,
    }


def write_manifest(path: Path, payload: dict) -> None:
    path.write_text(json.dumps(payload), encoding="utf-8")


def update_identity(manifest: dict, filename: str, path: Path) -> None:
    record = next(item for item in manifest["artifacts"] if item["filename"] == filename)
    record["bytes"] = path.stat().st_size
    record["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()


def make_release(directory: Path) -> tuple[Path, dict]:
    provenance = expansion_provenance()
    contract = embedding_contract()
    files = {
        BASE_FILENAME: make_base_pack(),
        EXPAND_FILENAME: make_expand(provenance),
        EXPAND_PROVENANCE_FILENAME: provenance,
        OVERLAY_FILENAME: make_overlay(),
        DENSE_FILENAME: make_dense_pack(contract["embedding_space_id"]),
        CONTRACT_FILENAME: json.dumps(contract).encode(),
    }
    for filename, payload in files.items():
        (directory / filename).write_bytes(payload)
    manifest = {
        "schema": SCHEMA,
        "canonical_translation": "BSB",
        "canonical_verse_count": VERSE_COUNT,
        "artifacts": [
            identity_record(directory / BASE_FILENAME, BASE_ROLE, True),
            identity_record(directory / EXPAND_FILENAME, EXPAND_ROLE, False),
            identity_record(
                directory / EXPAND_PROVENANCE_FILENAME,
                EXPAND_PROVENANCE_ROLE,
                False,
            ),
            identity_record(directory / OVERLAY_FILENAME, OVERLAY_ROLE, False),
            identity_record(
                directory / DENSE_FILENAME,
                DENSE_ROLE,
                False,
                lvec_version=2,
                dimensions=256,
                table_count=1,
                embedding_space_id=contract["embedding_space_id"],
                contract=CONTRACT_FILENAME,
            ),
            identity_record(directory / CONTRACT_FILENAME, CONTRACT_ROLE, False),
        ],
    }
    manifest_path = directory / "manifest.json"
    write_manifest(manifest_path, manifest)
    return manifest_path, manifest


class VerifyReleaseArtifactsTests(unittest.TestCase):
    def test_accepts_complete_exact_release_and_rejects_content_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, _ = make_release(directory)
            self.assertEqual(len(verify(manifest_path, directory)), 6)

            base = directory / BASE_FILENAME
            base.write_bytes(base.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "expected .* bytes"):
                verify(manifest_path, directory)

    def test_rejects_one_file_arbitrary_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            artifact = directory / "anything.pack"
            artifact.write_bytes(b"not-a-release")
            manifest = directory / "manifest.json"
            write_manifest(
                manifest,
                {
                    "schema": SCHEMA,
                    "canonical_translation": "BSB",
                    "canonical_verse_count": VERSE_COUNT,
                    "artifacts": [
                        identity_record(artifact, "anything", True),
                    ],
                },
            )
            with self.assertRaisesRegex(ValueError, "complete six-artifact set"):
                verify(manifest, directory)

    def test_rejects_unknown_missing_and_duplicate_roles(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, original = make_release(directory)
            cases: list[tuple[str, dict, str]] = []

            unknown = copy.deepcopy(original)
            unknown["artifacts"][0]["role"] = "mystery-role"
            cases.append(("unknown", unknown, "unknown or missing role"))

            missing = copy.deepcopy(original)
            missing["artifacts"].pop()
            cases.append(("missing", missing, "complete six-artifact set"))

            duplicate = copy.deepcopy(original)
            duplicate["artifacts"][2] = copy.deepcopy(duplicate["artifacts"][1])
            cases.append(("duplicate", duplicate, "duplicate release role"))

            for name, payload, pattern in cases:
                with self.subTest(name=name):
                    write_manifest(manifest_path, payload)
                    with self.assertRaisesRegex(ValueError, pattern):
                        verify(manifest_path, directory)

    def test_rejects_wrong_filename_required_flags_and_unknown_keys(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, original = make_release(directory)
            cases: list[tuple[str, dict, str]] = []

            wrong_filename = copy.deepcopy(original)
            wrong_filename["artifacts"][0]["filename"] = "other.pack"
            cases.append(("filename", wrong_filename, "invalid release identity"))

            wrong_required = copy.deepcopy(original)
            wrong_required["artifacts"][0]["required"] = False
            cases.append(("required", wrong_required, "invalid release identity"))

            unknown_artifact_key = copy.deepcopy(original)
            unknown_artifact_key["artifacts"][0]["comment"] = "trust me"
            cases.append(("artifact-key", unknown_artifact_key, "unsupported fields"))

            unknown_manifest_key = copy.deepcopy(original)
            unknown_manifest_key["profile"] = "weaker"
            cases.append(("manifest-key", unknown_manifest_key, "unsupported fields"))

            for name, payload, pattern in cases:
                with self.subTest(name=name):
                    write_manifest(manifest_path, payload)
                    with self.assertRaisesRegex(ValueError, pattern):
                        verify(manifest_path, directory)

    def test_rejects_duplicate_json_keys(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, manifest = make_release(directory)
            manifest_path.write_text(
                '{"schema":"%s","schema":"%s","canonical_translation":"BSB",'
                '"canonical_verse_count":31086,"artifacts":%s}'
                % (SCHEMA, SCHEMA, json.dumps(manifest["artifacts"])),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
                verify(manifest_path, directory)

    def test_rejects_structurally_invalid_base_pack_even_when_rehashed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, manifest = make_release(directory)
            base = directory / BASE_FILENAME
            payload = bytearray(base.read_bytes())
            position = payload.find(b"LEX1")
            self.assertGreater(position, 0)
            payload[position : position + 4] = b"BAD!"
            base.write_bytes(payload)
            update_identity(manifest, BASE_FILENAME, base)
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "invalid LEX section"):
                verify(manifest_path, directory)

    def test_rejects_every_mismatched_normalizer_identity_even_when_rehashed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)

            manifest_path, manifest = make_release(directory)
            base = directory / BASE_FILENAME
            payload = bytearray(base.read_bytes())
            position = payload.find(b"LEX1")
            self.assertGreater(position, 0)
            payload[position + 6 : position + 8] = (1).to_bytes(2, "little")
            base.write_bytes(payload)
            update_identity(manifest, BASE_FILENAME, base)
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "unsupported LEX version, normalizer"):
                verify(manifest_path, directory)

            manifest_path, manifest = make_release(directory)
            expand = directory / EXPAND_FILENAME
            payload = bytearray(expand.read_bytes())
            position = payload.find(b"VREXP001")
            self.assertGreater(position, 0)
            payload[position + 10 : position + 12] = (1).to_bytes(2, "little")
            expand.write_bytes(payload)
            update_identity(manifest, EXPAND_FILENAME, expand)
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "EXPAND header"):
                verify(manifest_path, directory)

            manifest_path, manifest = make_release(directory)
            provenance = directory / EXPAND_PROVENANCE_FILENAME
            changed = json.loads(provenance.read_text(encoding="utf-8"))
            changed["provenance"]["normalizer_version"] = 1
            provenance.write_text(json.dumps(changed), encoding="utf-8")
            expand = directory / EXPAND_FILENAME
            payload = bytearray(expand.read_bytes())
            position = payload.find(b"VREXP001")
            self.assertGreater(position, 0)
            payload[position + 40 : position + 72] = hashlib.sha256(
                provenance.read_bytes()
            ).digest()
            expand.write_bytes(payload)
            update_identity(manifest, EXPAND_FILENAME, expand)
            update_identity(manifest, EXPAND_PROVENANCE_FILENAME, provenance)
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "provenance contains invalid values"):
                verify(manifest_path, directory)

    def test_rejects_legacy_expand_and_unbound_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, original = make_release(directory)

            expand = directory / EXPAND_FILENAME
            payload = bytearray(expand.read_bytes())
            position = payload.find(b"VREXP001")
            self.assertGreater(position, 0)
            payload[position + 8 : position + 10] = (1).to_bytes(2, "little")
            expand.write_bytes(payload)
            legacy_manifest = copy.deepcopy(original)
            update_identity(legacy_manifest, EXPAND_FILENAME, expand)
            write_manifest(manifest_path, legacy_manifest)
            with self.assertRaisesRegex(ValueError, "EXPAND header"):
                verify(manifest_path, directory)

            manifest_path, manifest = make_release(directory)
            provenance = directory / EXPAND_PROVENANCE_FILENAME
            changed = json.loads(provenance.read_text(encoding="utf-8"))
            changed["provenance"]["builder"] = "different-general-builder"
            provenance.write_text(json.dumps(changed), encoding="utf-8")
            update_identity(manifest, EXPAND_PROVENANCE_FILENAME, provenance)
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "embedded provenance SHA-256"):
                verify(manifest_path, directory)

    def test_rejects_untyped_overlay_even_when_rehashed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, manifest = make_release(directory)
            overlay = directory / OVERLAY_FILENAME
            payload = bytearray(overlay.read_bytes())
            position = payload.find(b"VROVL001")
            self.assertGreater(position, 0)
            payload[position : position + 8] = b"UNTYPED!"
            overlay.write_bytes(payload)
            update_identity(manifest, OVERLAY_FILENAME, overlay)
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "typed PHRASE overlay magic"):
                verify(manifest_path, directory)

    def test_dense_artifact_requires_matching_fixed_content_addressed_contract(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest_path, manifest = make_release(directory)
            dense = next(item for item in manifest["artifacts"] if item["role"] == DENSE_ROLE)
            dense["embedding_space_id"] = "0" * 64
            write_manifest(manifest_path, manifest)
            with self.assertRaisesRegex(ValueError, "fixed Arctic-M256 contract"):
                verify(manifest_path, directory)


if __name__ == "__main__":
    unittest.main()
