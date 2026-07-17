#!/usr/bin/env python3
"""Mine a deterministic, corpus-targeted EXPAND vocabulary graph.

The input embedding matrix is a NumPy ``.npy`` array whose rows align with an
explicit vocabulary file or, by default, the sorted unique ``[a-z]+`` tokens
reconstructed from the canonical JSONL ``text`` fields.  Only build-time
NumPy is required; the emitted JSONL is consumed by ``recall-packer expand``.

Example::

    python3 scripts/build_expand_neighbors.py \
      --canonical data/bsb.jsonl \
      --embeddings /private/tmp/bge-bsb-vocab.npy \
      --output /private/tmp/bsb-expand.jsonl \
      --canonical-translation BSB
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Iterable, Sequence

import numpy as np


SCHEMA_VERSION = 1
BUILDER_ID = "verse-recall-expand-neighbors-v1"
NORMALIZER_VERSION = 2
ASCII_TOKEN = re.compile(r"[a-z]+")
ASCII_TOKEN_FULL = re.compile(r"[a-z]+\Z")
MAX_TARGETS = 64

# Keep this aligned with recall-core's query stop set. Closed-class terms are
# poor semantic graph nodes and many are removed before Lane B sees a query.
STOP_WORDS = frozenset(
    {
        "a",
        "an",
        "the",
        "and",
        "or",
        "but",
        "if",
        "of",
        "to",
        "in",
        "on",
        "for",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "that",
        "this",
        "it",
        "he",
        "she",
        "they",
        "we",
        "you",
        "i",
        "his",
        "her",
        "their",
        "my",
        "your",
        "with",
        "as",
        "by",
        "from",
        "at",
        "not",
        "no",
        "so",
        "one",
        "only",
        "about",
        "verse",
        "verses",
        "passage",
        "passages",
        "scripture",
        "bible",
        "guy",
        "man",
        "people",
        "person",
        "someone",
        "where",
        "when",
        "who",
        "what",
        "how",
        "which",
        "there",
        "here",
        "into",
        "over",
        "under",
        "than",
        "then",
        "also",
        "just",
        "very",
        "much",
        "more",
        "some",
        "any",
        "all",
        "its",
        "our",
        "does",
        "did",
        "do",
        "has",
        "have",
        "had",
        "will",
        "would",
        "could",
        "should",
        "can",
        "may",
        "might",
        "shall",
        "unto",
        "ye",
        "thou",
        "thy",
        "thee",
    }
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_vocabulary(vocabulary: Sequence[str]) -> str:
    payload = "".join(f"{token}\n" for token in vocabulary).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def light_stem(token: str) -> str:
    """Mirror recall-core's deliberately small English stemmer.

    EXPAND JSONL may contain surface forms, but pre-stemming here prevents a
    raw nearest-neighbor list from spending its 64-edge budget on inflections.
    ``recall-packer`` remains the authority and normalizes these values again.
    """

    irregular = {
        "lost": "lose",
        "loses": "lose",
        "lose": "lose",
        "losing": "lose",
        "losts": "lose",
        "loved": "love",
        "loves": "love",
        "loving": "love",
        "love": "love",
        "gave": "give",
        "gives": "give",
        "giving": "give",
        "give": "give",
        "given": "give",
        "found": "find",
        "finds": "find",
        "finding": "find",
        "find": "find",
        "left": "leave",
        "leaves": "leave",
        "leaving": "leave",
        "leave": "leave",
        "blessed": "bless",
        "blesses": "bless",
        "blessing": "bless",
        "bless": "bless",
        "believes": "believe",
        "believed": "believe",
        "believing": "believe",
        "believe": "believe",
        "believers": "believe",
        "believer": "believe",
        "created": "create",
        "creates": "create",
        "creating": "create",
        "create": "create",
        "creation": "create",
        "made": "make",
        "make": "make",
        "makes": "make",
        "making": "make",
        "shepherds": "shepherd",
        "shepherding": "shepherd",
        "shepherded": "shepherd",
        "shepherd": "shepherd",
        "sheep": "sheep",
        "sheeps": "sheep",
        "sons": "son",
        "son": "son",
        "worlds": "world",
        "world": "world",
        "spirits": "spirit",
        "spirit": "spirit",
        "moneys": "money",
        "money": "money",
        "evils": "evil",
        "evil": "evil",
        "roots": "root",
        "root": "root",
        "poor": "poor",
        "begotten": "begot",
        "ninety": "ninety",
        "ninty": "ninety",
        "hundred": "hundred",
        "parable": "parable",
        "parables": "parable",
        "kingdom": "kingdom",
        "kingdoms": "kingdom",
        "heaven": "heaven",
        "heavens": "heaven",
        "eternal": "eternal",
        "eternally": "eternal",
        "perish": "perish",
        "perished": "perish",
        "perishes": "perish",
        "perishing": "perish",
        "walking": "walk",
        "walked": "walk",
        "walks": "walk",
        "walk": "walk",
        # These intentionally mirror recall-core exactly.  Semantic
        # relationships such as afraid -> fear belong in the mined graph;
        # the normalizer must not invent them or build/runtime identities
        # diverge.
        "scared": "scare",
        "scares": "scare",
        "scaring": "scare",
        "scare": "scare",
        "scary": "scary",
        "afraid": "afraid",
        "dangerous": "danger",
        "danger": "danger",
        "darkness": "dark",
        "dark": "dark",
        "qualities": "quality",
        "quality": "quality",
        "faithful": "faithful",
        "faithfully": "faithful",
        "faithfulness": "faithful",
        "angry": "anger",
        "anger": "anger",
        "produces": "produce",
        "produced": "produce",
        "producing": "produce",
        "produce": "produce",
        "beginning": "begin",
        "begin": "begin",
        "began": "begin",
        "begun": "begin",
        "starts": "start",
        "started": "start",
        "starting": "start",
        "start": "start",
        "armor": "armor",
        "armour": "armor",
        "psalm": "psalm",
        "psalms": "psalm",
    }
    if token in irregular:
        return irregular[token]
    if token in {"everything", "everyone", "everyone's"}:
        return token
    if token.endswith(
        ("thing", "ness", "ous", "tion", "sion", "ment", "ity", "ence", "ance")
    ):
        return token

    stem = token
    if len(stem) > 6 and stem.endswith("ing"):
        base = stem[:-3]
        if len(base) >= 4:
            stem = base
            if len(stem) >= 2 and stem[-1] == stem[-2]:
                stem = stem[:-1]
    elif len(stem) > 5 and stem.endswith("ies"):
        stem = stem[:-3] + "y"
    elif len(stem) > 5 and stem.endswith("es") and not stem.endswith(
        ("ness", "ous")
    ):
        base = stem[:-2]
        if base.endswith(("s", "x", "z", "ch", "sh")):
            stem = base
    elif (
        len(stem) > 4
        and stem.endswith("s")
        and not stem.endswith(("ss", "us"))
    ):
        stem = stem[:-1]
    elif len(stem) > 5 and stem.endswith("ed"):
        stem = stem[:-2]
        if stem.endswith("i"):
            stem = stem[:-1] + "y"
    elif len(stem) > 5 and stem.endswith("ly"):
        stem = stem[:-2]
    return stem


def read_canonical_corpus(path: Path, text_field: str = "text") -> tuple[int, list[str]]:
    verse_count = 0
    vocabulary: set[str] = set()
    with path.open("r", encoding="utf-8") as source:
        for line_number, line in enumerate(source, 1):
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"{path}:{line_number}: invalid JSON: {error}") from error
            if not isinstance(value, dict) or not isinstance(value.get(text_field), str):
                raise ValueError(
                    f"{path}:{line_number}: expected an object with string {text_field!r}"
                )
            verse_count += 1
            vocabulary.update(ASCII_TOKEN.findall(value[text_field].lower()))
    if verse_count == 0 or not vocabulary:
        raise ValueError(f"{path}: canonical corpus has no text rows")
    return verse_count, sorted(vocabulary)


def read_vocabulary(path: Path) -> list[str]:
    raw = path.read_text(encoding="utf-8")
    if raw.lstrip().startswith("["):
        value = json.loads(raw)
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            raise ValueError(f"{path}: vocabulary JSON must be an array of strings")
        vocabulary = value
    else:
        vocabulary = [
            line.strip()
            for line in raw.splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        ]
    if not vocabulary:
        raise ValueError(f"{path}: vocabulary is empty")
    seen: set[str] = set()
    for row, token in enumerate(vocabulary, 1):
        if ASCII_TOKEN_FULL.fullmatch(token) is None:
            raise ValueError(
                f"{path}:{row}: vocabulary tokens must be lowercase ASCII [a-z]+"
            )
        if token in seen:
            raise ValueError(f"{path}:{row}: duplicate vocabulary token {token!r}")
        seen.add(token)
    return vocabulary


def normalize_matrix(matrix: np.ndarray) -> np.ndarray:
    if not isinstance(matrix, np.ndarray) or matrix.ndim != 2:
        raise ValueError("embeddings must be a two-dimensional NumPy array")
    if matrix.shape[0] == 0 or matrix.shape[1] == 0:
        raise ValueError("embeddings must have at least one row and dimension")
    if matrix.dtype.kind not in "fiu":
        raise ValueError("embeddings must have a numeric dtype")
    values = np.asarray(matrix, dtype=np.float32, order="C")
    if not np.all(np.isfinite(values)):
        raise ValueError("embeddings contain a nonfinite value")
    norms = np.sqrt(np.einsum("ij,ij->i", values, values, optimize=False))
    if np.any(~np.isfinite(norms)) or np.any(norms <= 1e-12):
        raise ValueError("embeddings contain a nonfinite or zero-norm row")
    return np.asarray(values / norms[:, None], dtype=np.float32, order="C")


def calibrate_weight(similarity: float, gamma: float) -> float:
    """Clamp normalized cosine into EXPAND's ``(0, 1]`` weight interval."""

    weight = min(1.0, max(0.0, similarity)) ** gamma
    # Stable decimal output also avoids meaningless float noise in reviews.
    return round(weight, 6)


def batched(values: Sequence[int], size: int) -> Iterable[Sequence[int]]:
    for start in range(0, len(values), size):
        yield values[start : start + size]


def mine_expansions(
    vocabulary: Sequence[str],
    canonical_vocabulary: Sequence[str],
    normalized_vectors: np.ndarray,
    *,
    top_k: int = MAX_TARGETS,
    similarity_floor: float = 0.64,
    batch_size: int = 256,
    weight_gamma: float = 1.0,
) -> dict[str, list[tuple[str, float]]]:
    if not 1 <= top_k <= MAX_TARGETS:
        raise ValueError(f"top_k must be between 1 and {MAX_TARGETS}")
    if not 0.0 < similarity_floor < 1.0:
        raise ValueError("similarity_floor must be in (0, 1)")
    if batch_size <= 0:
        raise ValueError("batch_size must be positive")
    if not np.isfinite(weight_gamma) or weight_gamma <= 0.0:
        raise ValueError("weight_gamma must be finite and positive")
    if normalized_vectors.ndim != 2 or normalized_vectors.shape[0] != len(vocabulary):
        raise ValueError("embedding rows do not align with the vocabulary")

    stems = [light_stem(token) for token in vocabulary]
    canonical_stems = {
        light_stem(token)
        for token in canonical_vocabulary
        if token not in STOP_WORDS and len(light_stem(token)) > 1
    }
    source_ids = [
        index
        for index, token in enumerate(vocabulary)
        if token not in STOP_WORDS and len(stems[index]) > 1
    ]
    target_valid = np.asarray(
        [
            token not in STOP_WORDS
            and len(stems[index]) > 1
            and stems[index] in canonical_stems
            for index, token in enumerate(vocabulary)
        ],
        dtype=bool,
    )
    stem_rows: dict[str, list[int]] = {}
    for index, stemmed in enumerate(stems):
        stem_rows.setdefault(stemmed, []).append(index)
    # Tie-breaking must not depend on the order of an explicit vocabulary.
    lexical_rank = np.empty(len(vocabulary), dtype=np.int64)
    for rank, index in enumerate(sorted(range(len(vocabulary)), key=vocabulary.__getitem__)):
        lexical_rank[index] = rank

    merged: dict[str, dict[str, float]] = {}
    for source_batch in batched(source_ids, batch_size):
        batch_matrix = normalized_vectors[np.asarray(source_batch, dtype=np.int64)]
        # Some BLAS/NumPy combinations report spurious IEEE warnings during a
        # perfectly finite SGEMM. Suppress those at the primitive boundary and
        # enforce finiteness explicitly before applying our -inf masks.
        with np.errstate(divide="ignore", over="ignore", invalid="ignore"):
            scores = np.asarray(batch_matrix @ normalized_vectors.T, dtype=np.float32)
        if not np.all(np.isfinite(scores)):
            raise ValueError("cosine scoring produced a nonfinite value")
        scores[:, ~target_valid] = -np.inf
        for batch_row, source_index in enumerate(source_batch):
            source_stem = stems[source_index]
            row = scores[batch_row]
            row[stem_rows[source_stem]] = -np.inf
            candidates = np.flatnonzero(row > similarity_floor)
            if candidates.size == 0:
                continue
            order = np.lexsort((lexical_rank[candidates], -row[candidates]))
            local: dict[str, float] = {}
            for target_index in candidates[order]:
                target_stem = stems[int(target_index)]
                if target_stem in local:
                    continue
                weight = calibrate_weight(float(row[target_index]), weight_gamma)
                if weight <= 0.0:
                    continue
                local[target_stem] = weight
                if len(local) == top_k:
                    break
            if not local:
                continue
            source_edges = merged.setdefault(source_stem, {})
            for target_stem, weight in local.items():
                source_edges[target_stem] = max(
                    weight, source_edges.get(target_stem, 0.0)
                )

    result: dict[str, list[tuple[str, float]]] = {}
    for source_stem, targets in sorted(merged.items()):
        ranked = sorted(targets.items(), key=lambda item: (-item[1], item[0]))[:top_k]
        if ranked:
            result[source_stem] = ranked
    return result


def build_expand_jsonl(
    *,
    canonical: Path,
    embeddings: Path,
    output: Path,
    canonical_translation: str = "BSB",
    vocabulary_path: Path | None = None,
    text_field: str = "text",
    top_k: int = MAX_TARGETS,
    similarity_floor: float = 0.64,
    batch_size: int = 256,
    weight_gamma: float = 1.0,
) -> tuple[int, int]:
    canonical_translation = canonical_translation.strip()
    if not canonical_translation or any(
        ord(char) < 32 or ord(char) == 127 for char in canonical_translation
    ):
        raise ValueError("canonical_translation must be a non-empty identifier")
    verse_count, canonical_vocabulary = read_canonical_corpus(canonical, text_field)
    vocabulary = (
        read_vocabulary(vocabulary_path)
        if vocabulary_path is not None
        else canonical_vocabulary
    )
    loaded = np.load(embeddings, mmap_mode="r", allow_pickle=False)
    if not isinstance(loaded, np.ndarray):
        raise ValueError(f"{embeddings}: expected one .npy array, not an archive")
    if loaded.shape[0] != len(vocabulary):
        raise ValueError(
            f"{embeddings}: {loaded.shape[0]} rows do not match "
            f"{len(vocabulary)} vocabulary terms"
        )
    vectors = normalize_matrix(loaded)
    mappings = mine_expansions(
        vocabulary,
        canonical_vocabulary,
        vectors,
        top_k=top_k,
        similarity_floor=similarity_floor,
        batch_size=batch_size,
        weight_gamma=weight_gamma,
    )
    edge_count = sum(len(targets) for targets in mappings.values())
    if not mappings or edge_count == 0:
        raise ValueError("no expansion edges survived filtering")

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "kind": "manifest",
        "canonical_translation": canonical_translation,
        "canonical_verse_count": verse_count,
        "provenance": {
            "builder": BUILDER_ID,
            "normalizer_version": NORMALIZER_VERSION,
            "canonical_sha256": sha256_file(canonical),
            "embeddings_sha256": sha256_file(embeddings),
            "vocabulary_sha256": sha256_vocabulary(vocabulary),
            "embedding_rows": int(vectors.shape[0]),
            "embedding_dimensions": int(vectors.shape[1]),
            "source_count": len(mappings),
            "edge_count": edge_count,
            "parameters": {
                "top_k": top_k,
                "similarity_floor": similarity_floor,
                "batch_size": batch_size,
                "weight_gamma": weight_gamma,
                "weight_calibration": "positive-cosine-power-v1",
            },
        },
    }
    lines = [json.dumps(manifest, sort_keys=True, separators=(",", ":"))]
    # The packer accepts vocabulary surfaces and applies the authoritative
    # stemmer once. Emitting our already-stemmed key is unsafe for the few
    # non-idempotent forms (for example accordings -> according -> accord), so
    # retain a deterministic original surface for every mined stem.
    representatives: dict[str, str] = {}
    for token in vocabulary:
        stemmed = light_stem(token)
        if token in STOP_WORDS or len(stemmed) <= 1:
            continue
        representatives[stemmed] = min(token, representatives.get(stemmed, token))
    for source_stem, targets in mappings.items():
        record = {
            "schema_version": SCHEMA_VERSION,
            "kind": "expansion",
            "source": representatives[source_stem],
            "targets": [
                {"stem": representatives[target_stem], "weight": weight}
                for target_stem, weight in targets
            ],
        }
        lines.append(json.dumps(record, sort_keys=True, separators=(",", ":")))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return len(mappings), edge_count


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--embeddings", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--canonical-translation", default="BSB")
    parser.add_argument(
        "--vocabulary",
        type=Path,
        help="optional row-aligned newline or JSON-array vocabulary",
    )
    parser.add_argument("--text-field", default="text")
    parser.add_argument("--top-k", type=int, default=MAX_TARGETS)
    parser.add_argument("--similarity-floor", type=float, default=0.64)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument(
        "--weight-gamma",
        type=float,
        default=1.0,
        help="positive cosine calibration exponent (default: preserve cosine)",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        source_count, edge_count = build_expand_jsonl(
            canonical=args.canonical,
            embeddings=args.embeddings,
            output=args.output,
            canonical_translation=args.canonical_translation,
            vocabulary_path=args.vocabulary,
            text_field=args.text_field,
            top_k=args.top_k,
            similarity_floor=args.similarity_floor,
            batch_size=args.batch_size,
            weight_gamma=args.weight_gamma,
        )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(
        f"wrote {source_count} source mappings / {edge_count} edges -> {args.output}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
