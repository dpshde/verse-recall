#!/usr/bin/env python3
"""Build reproducible corpus or query vectors for the optional LVEC lane.

Dependencies are intentionally build/host-side only:
    python -m pip install numpy onnxruntime tokenizers
"""

from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path
import re
import struct
from typing import Iterable

import numpy as np


PROJECTION_MAGIC = b"VRPROJ01"
PROJECTION_HEADER = struct.Struct("<8sII")
QUERY_ADAPTER_MAGIC = b"VRQADP01"
QUERY_ADAPTER_HEADER = struct.Struct("<8sII")
SPACE_CONTRACT_SCHEMA = "verse-recall-embedding-space-v1"
QUERY_SEGMENT_PLANNER = "verse-recall-clause-v2"
PROMPT_UTF8_SHA256_FIELD = "prompt_utf8_sha256"
MAX_QUERY_SEGMENTS = 4

# These are grammatical glue words rather than concepts. The planner uses the
# list only to decide whether both sides of a boundary carry enough information
# to stand alone; it never rewrites a query or associates words with passages.
_FUNCTION_WORDS = {
    "a",
    "about",
    "an",
    "and",
    "are",
    "as",
    "at",
    "be",
    "been",
    "being",
    "but",
    "by",
    "did",
    "do",
    "does",
    "for",
    "from",
    "had",
    "has",
    "have",
    "he",
    "her",
    "hers",
    "him",
    "his",
    "how",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "me",
    "my",
    "nor",
    "not",
    "of",
    "on",
    "or",
    "our",
    "ours",
    "she",
    "so",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "to",
    "us",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "who",
    "why",
    "will",
    "with",
    "would",
    "you",
    "your",
    "yours",
}
_WORD_RE = re.compile(r"[^\W_]+(?:['\u2019][^\W_]+)*", re.UNICODE)
_QUERY_BOUNDARY_RE = re.compile(
    r"(?P<punct>[;!?]+|\.(?=\s+|$)|[\u2013\u2014])"
    r"|(?P<coord>(?<!\w)(?:as\s+well\s+as|and\s+then|do\s+not|and|but|or|yet|"
    r"whereas|because|while|plus)(?!\w))",
    re.IGNORECASE,
)
_COMMON_ABBREVIATION_RE = re.compile(
    r"(?:\b(?:mr|mrs|ms|dr|st|rev|vs|cf|etc|no)|\b(?:e\.g|i\.e))\.$",
    re.IGNORECASE,
)
_DISPLAY_REFERENCE_RE = re.compile(
    r"^(?P<book>.+?) (?P<chapter>[1-9][0-9]*):(?P<verse>[1-9][0-9]*)$"
)


def _query_words(text: str) -> list[str]:
    return [match.group(0) for match in _WORD_RE.finditer(text)]


def _meaningful_query_word_count(text: str) -> int:
    return sum(
        1
        for word in _query_words(text)
        if any(character.isalpha() for character in word)
        and word.casefold() not in _FUNCTION_WORDS
    )


def _independently_useful_query_segment(text: str, meaningful: int) -> bool:
    words = _query_words(text)
    if len(words) < 2 or meaningful == 0:
        return False
    if meaningful >= 2:
        return True
    # Determiner + content noun is a complete noun phrase; a negated or
    # prepositional tail with one content word remains a fragment.
    return words[0].casefold() in {"a", "an", "the"}


def _clean_query_segment(text: str) -> str:
    # Work in Python Unicode strings throughout: no byte offsets are ever used.
    return re.sub(r"\s+", " ", text).strip(" \t\r\n,;:.!?\u2013\u2014")


def _query_boundary_relation(match: re.Match[str]) -> tuple[str, bool]:
    """Classify a grammatical boundary without consulting corpus content.

    The boolean says that the marker belongs to the right-hand clause. This
    preserves negation for ``do not`` while keeping the public planner payload
    as the same ordered string list used by every host.
    """

    if match.group("punct") is not None:
        return "strong", False
    connector = re.sub(r"\s+", " ", match.group("coord").casefold())
    if connector in {"as well as", "and then", "and", "plus"}:
        return "additive", False
    if connector == "or":
        return "alternative", False
    if connector in {"but", "yet", "whereas"}:
        return "contrast", False
    if connector == "do not":
        return "contrast", True
    if connector in {"because", "while"}:
        return "subordinate", False
    raise AssertionError(f"unclassified query boundary {connector!r}")


def plan_query_segments(query: str) -> list[str]:
    """Return conservative, query-only clause candidates for dense retrieval.

    Boundaries are selected without corpus, evaluation, or gold-reference
    access. Every returned side must contain at least two non-function words,
    which avoids fragmenting ordinary phrases such as ``faith and hope``.
    At most three of the first 24 plausible boundaries are considered, keeping
    planning bounded even for accidentally pasted paragraphs.
    """

    query = re.sub(r"\s+", " ", query).strip()
    if not query:
        return []

    # start, end, next-segment start, relation
    boundaries: list[tuple[int, int, int, str]] = []
    for match in _QUERY_BOUNDARY_RE.finditer(query):
        relation, retain_right = _query_boundary_relation(match)
        if relation == "strong" and match.group("punct") == ".":
            if _COMMON_ABBREVIATION_RE.search(query[: match.end()]):
                continue
        next_start = match.start() if retain_right else match.end()
        if not _clean_query_segment(query[: match.start()]) or not _clean_query_segment(
            query[next_start:]
        ):
            continue
        boundaries.append((match.start(), match.end(), next_start, relation))
        if len(boundaries) > 24:
            # A normal search query should never reach this point. Refusing to
            # decompose a pasted paragraph is safer than selecting arbitrary
            # fragments from it.
            return []

    if not boundaries:
        return []
    if (
        sum(relation == "strong" for _, _, _, relation in boundaries)
        >= MAX_QUERY_SEGMENTS
    ):
        # More than four independently punctuated clauses cannot be represented
        # without silently dropping or re-combining an intent.
        return []

    candidates: list[tuple[tuple, list[str]]] = []
    for boundary_count in range(1, min(MAX_QUERY_SEGMENTS - 1, len(boundaries)) + 1):
        for selected in itertools.combinations(boundaries, boundary_count):
            segments = []
            start = 0
            for boundary_start, _boundary_end, next_start, _relation in selected:
                segments.append(_clean_query_segment(query[start:boundary_start]))
                start = next_start
            segments.append(_clean_query_segment(query[start:]))
            meaningful_counts = [
                _meaningful_query_word_count(segment) for segment in segments
            ]
            if any(
                not _independently_useful_query_segment(segment, meaningful)
                for segment, meaningful in zip(segments, meaningful_counts)
            ):
                continue
            strong_count = sum(
                relation == "strong" for _, _, _, relation in selected
            )
            independent_count = sum(
                relation in {"strong", "additive", "contrast"}
                for _, _, _, relation in selected
            )
            imbalance = max(meaningful_counts) - min(meaningful_counts)
            # Strong punctuation outranks coordination; within that constraint,
            # retain more independently meaningful clauses and prefer balance.
            score = (
                strong_count,
                independent_count,
                len(segments),
                sum(meaningful_counts),
                -imbalance,
                tuple(-boundary_start for boundary_start, _, _, _ in selected),
            )
            candidates.append((score, segments))

    if not candidates:
        return []
    return max(candidates, key=lambda candidate: candidate[0])[1]


def read_jsonl(path: Path) -> list[dict]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError(f"{path}:{line_number}: expected a JSON object")
        rows.append(value)
    return rows


def batched(values: list[str], size: int) -> Iterable[list[str]]:
    for start in range(0, len(values), size):
        yield values[start : start + size]


def _reference_chapter(reference: object, row_number: int) -> tuple[str, int]:
    if not isinstance(reference, str):
        raise ValueError(
            f"corpus row {row_number} requires a canonical ref when building windows"
        )
    match = _DISPLAY_REFERENCE_RE.fullmatch(reference.strip())
    if match is None:
        raise ValueError(
            f"corpus row {row_number} has invalid canonical ref {reference!r}"
        )
    return match.group("book").casefold(), int(match.group("chapter"))


def chapter_bounded_windows(rows: list[dict], texts: list[str]) -> list[str]:
    """Build verse +/-1 windows without leaking across canonical chapters.

    A row remains the anchor for its window. Runtime retrieval attaches the
    identical range as passage provenance, so text used for similarity and the
    result shown to callers cannot silently disagree.
    """

    if len(rows) != len(texts):
        raise ValueError("corpus rows and canonical texts must have equal length")
    chapters = [
        _reference_chapter(row.get("ref"), index)
        for index, row in enumerate(rows, 1)
    ]
    windows = []
    for index in range(len(texts)):
        start = index
        if index > 0 and chapters[index - 1] == chapters[index]:
            start -= 1
        end = index + 1
        if index + 1 < len(texts) and chapters[index + 1] == chapters[index]:
            end += 1
        windows.append(" ".join(texts[start:end]))
    return windows


def normalize_vectors(vectors: np.ndarray) -> np.ndarray:
    vectors = np.asarray(vectors, dtype=np.float32)
    norms = np.linalg.norm(vectors, axis=1, keepdims=True)
    if not np.all(np.isfinite(vectors)) or np.any(norms <= 1e-12):
        raise ValueError("vectors contain a nonfinite or zero-norm row")
    return (vectors / norms).astype("<f4", copy=False)


def read_f32_matrix(path: Path, dims: int) -> np.ndarray:
    if dims <= 0:
        raise ValueError("vector dimensions must be positive")
    values = np.fromfile(path, dtype="<f4")
    if values.size == 0 or values.size % dims:
        raise ValueError(f"{path} is not a non-empty row-major {dims}D f32 matrix")
    matrix = values.reshape(-1, dims)
    if not np.all(np.isfinite(matrix)):
        raise ValueError(f"{path} contains nonfinite values")
    return matrix


def fit_pca_projection(
    vectors: np.ndarray, output_dims: int
) -> tuple[np.ndarray, np.ndarray]:
    """Fit centered PCA with a canonical sign for every component.

    Eigenvector signs are mathematically arbitrary. Anchoring each component's
    largest-magnitude coefficient positive makes repeated builds deterministic
    for a fixed NumPy/LAPACK implementation. The serialized projection artifact
    is subsequently content-addressed in the embedding-space ID.
    """

    vectors = np.asarray(vectors, dtype=np.float64)
    if vectors.ndim != 2 or vectors.shape[0] < 2:
        raise ValueError("projection fitting requires at least two vector rows")
    if not 0 < output_dims <= vectors.shape[1]:
        raise ValueError("output dimensions must be between 1 and input dimensions")
    if not np.all(np.isfinite(vectors)):
        raise ValueError("projection input contains nonfinite values")
    mean = vectors.mean(axis=0)
    centered = vectors - mean
    covariance = centered.T @ centered
    eigenvalues, eigenvectors = np.linalg.eigh(covariance)
    order = np.argsort(-eigenvalues, kind="stable")[:output_dims]
    components = eigenvectors[:, order]
    for column in range(components.shape[1]):
        pivot = int(np.argmax(np.abs(components[:, column])))
        if components[pivot, column] < 0:
            components[:, column] *= -1
    return mean.astype("<f4"), components.astype("<f4")


def save_projection(path: Path, mean: np.ndarray, components: np.ndarray) -> None:
    mean = np.asarray(mean, dtype="<f4")
    components = np.asarray(components, dtype="<f4")
    if (
        mean.ndim != 1
        or components.ndim != 2
        or components.shape[0] != mean.shape[0]
        or components.shape[1] == 0
        or not np.all(np.isfinite(mean))
        or not np.all(np.isfinite(components))
    ):
        raise ValueError("invalid projection mean or component matrix")
    header = PROJECTION_HEADER.pack(
        PROJECTION_MAGIC, components.shape[0], components.shape[1]
    )
    path.write_bytes(header + mean.tobytes(order="C") + components.tobytes(order="C"))


def load_projection(path: Path) -> tuple[np.ndarray, np.ndarray]:
    raw = path.read_bytes()
    if len(raw) < PROJECTION_HEADER.size:
        raise ValueError(f"{path} has a truncated projection header")
    magic, input_dims, output_dims = PROJECTION_HEADER.unpack_from(raw)
    if magic != PROJECTION_MAGIC or input_dims == 0 or output_dims == 0:
        raise ValueError(f"{path} has invalid projection metadata")
    value_count = input_dims + input_dims * output_dims
    expected = PROJECTION_HEADER.size + value_count * np.dtype("<f4").itemsize
    if len(raw) != expected:
        raise ValueError(f"{path} projection length does not match its dimensions")
    values = np.frombuffer(raw, dtype="<f4", offset=PROJECTION_HEADER.size)
    mean = values[:input_dims].copy()
    components = values[input_dims:].reshape(input_dims, output_dims).copy()
    if not np.all(np.isfinite(values)):
        raise ValueError(f"{path} projection contains nonfinite values")
    return mean, components


def apply_projection(
    vectors: np.ndarray,
    projection: tuple[np.ndarray, np.ndarray] | None,
    truncate_dims: int | None = None,
    pre_truncate_layer_norm: bool = False,
) -> np.ndarray:
    vectors = np.asarray(vectors, dtype=np.float32)
    if pre_truncate_layer_norm:
        # Some Matryoshka encoders (notably Nomic v1.5) train prefixes after
        # per-row layer normalization. This is representation metadata, not a
        # query heuristic, and must be identical for corpus and query vectors.
        mean = vectors.mean(axis=1, keepdims=True, dtype=np.float64)
        variance = np.mean(
            (vectors.astype(np.float64) - mean) ** 2,
            axis=1,
            keepdims=True,
        )
        vectors = ((vectors - mean) / np.sqrt(variance + 1e-5)).astype(np.float32)
    vectors = normalize_vectors(vectors)
    if truncate_dims is not None:
        if projection is not None:
            raise ValueError("truncate-dims cannot be combined with a projection")
        if not 0 < truncate_dims <= vectors.shape[1]:
            raise ValueError(
                "truncate dimensions must be between 1 and encoder output dimensions"
            )
        # Matryoshka encoders are trained so that an embedding prefix remains a
        # useful representation. Normalize the complete encoder output first,
        # then select that prefix and normalize it again for cosine retrieval.
        return normalize_vectors(vectors[:, :truncate_dims])
    if projection is None:
        return vectors
    mean, components = projection
    if vectors.shape[1] != components.shape[0] or mean.shape != (vectors.shape[1],):
        raise ValueError(
            f"projection requires {components.shape[0]}D input, got {vectors.shape[1]}D"
        )
    projected = (
        (vectors.astype(np.float64) - mean.astype(np.float64))
        @ components.astype(np.float64)
    ).astype(np.float32)
    return normalize_vectors(projected)


def save_query_adapter(path: Path, matrix: np.ndarray) -> None:
    """Write a content-addressable query-only linear adapter.

    Unlike ``VRPROJ01``, this transform is intentionally asymmetric: hosts
    apply it after query encoding while canonical corpus vectors remain in the
    encoder's document space.  That distinction is encoded in both the magic
    and the embedding-space contract so the two artifacts cannot be confused.
    """

    matrix = np.asarray(matrix, dtype="<f4")
    if (
        matrix.ndim != 2
        or min(matrix.shape) == 0
        or not np.all(np.isfinite(matrix))
    ):
        raise ValueError("invalid query-adapter matrix")
    header = QUERY_ADAPTER_HEADER.pack(
        QUERY_ADAPTER_MAGIC, matrix.shape[0], matrix.shape[1]
    )
    path.write_bytes(header + matrix.tobytes(order="C"))


def load_query_adapter(path: Path) -> np.ndarray:
    raw = path.read_bytes()
    if len(raw) < QUERY_ADAPTER_HEADER.size:
        raise ValueError(f"{path} has a truncated query-adapter header")
    magic, input_dims, output_dims = QUERY_ADAPTER_HEADER.unpack_from(raw)
    if magic != QUERY_ADAPTER_MAGIC or input_dims == 0 or output_dims == 0:
        raise ValueError(f"{path} has invalid query-adapter metadata")
    expected = (
        QUERY_ADAPTER_HEADER.size
        + input_dims * output_dims * np.dtype("<f4").itemsize
    )
    if len(raw) != expected:
        raise ValueError(f"{path} query-adapter length does not match its dimensions")
    matrix = np.frombuffer(
        raw, dtype="<f4", offset=QUERY_ADAPTER_HEADER.size
    ).reshape(input_dims, output_dims)
    if not np.all(np.isfinite(matrix)):
        raise ValueError(f"{path} query adapter contains nonfinite values")
    return matrix.copy()


def apply_query_adapter(vectors: np.ndarray, matrix: np.ndarray | None) -> np.ndarray:
    if matrix is None:
        # Preserve the established query byte stream when no adapter is
        # configured. Encoder.encode has already normalized real vectors; the
        # no-op is intentionally exact for compatibility and test doubles.
        return np.asarray(vectors, dtype="<f4")
    vectors = np.asarray(vectors, dtype=np.float32)
    matrix = np.asarray(matrix, dtype=np.float32)
    if vectors.ndim != 2:
        raise ValueError("query adapter input must be a rank-2 matrix")
    if matrix.ndim != 2:
        raise ValueError("query adapter must be a rank-2 matrix")
    if vectors.shape[1] != matrix.shape[0]:
        raise ValueError(
            f"query adapter requires {matrix.shape[0]}D input, got {vectors.shape[1]}D"
        )
    if not np.all(np.isfinite(vectors)) or not np.all(np.isfinite(matrix)):
        raise ValueError("query adapter input and matrix must contain finite values")

    # VRQADP01 arithmetic is portable by contract: normalize the Float32 input
    # in Float64, accumulate the row-major matrix product in Float64, normalize
    # again in Float64, then round once to Float32.  Swift and Kotlin use the
    # same sequence; keeping the intermediate out of Float32 prevents every
    # host from producing subtly different vectors after cancellation.
    vectors64 = vectors.astype(np.float64)
    input_norms = np.sqrt(np.sum(vectors64 * vectors64, axis=1, keepdims=True))
    if np.any(input_norms <= 0.0) or not np.all(np.isfinite(input_norms)):
        raise ValueError("vectors contain a nonfinite or zero-norm row")
    output64 = (vectors64 / input_norms) @ matrix.astype(np.float64)
    output_norms = np.sqrt(np.sum(output64 * output64, axis=1, keepdims=True))
    if np.any(output_norms <= 0.0) or not np.all(np.isfinite(output_norms)):
        raise ValueError("query adapter produced a nonfinite or zero-norm row")
    return (output64 / output_norms).astype("<f4")


def prompt_utf8_sha256(prompt: str) -> str:
    """Content-bind a query vector record to the exact decoded prompt text."""

    return hashlib.sha256(prompt.encode("utf-8")).hexdigest()


def validate_prompt_utf8_sha256(value: object, row_number: int) -> str:
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise ValueError(
            f"query row {row_number} requires a lowercase 64-hex "
            f"{PROMPT_UTF8_SHA256_FIELD}"
        )
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def embedding_space_contract(
    model: Path,
    tokenizer: Path,
    pooling: str,
    max_length: int,
    query_prefix: str,
    corpus_prefix: str,
    projection: Path | None,
    extra_model_assets: list[Path] | None = None,
    truncate_dims: int | None = None,
    query_adapter: Path | None = None,
    pre_truncate_layer_norm: bool = False,
) -> dict:
    if not model.is_file() or not tokenizer.is_file():
        raise ValueError(f"missing model or tokenizer: {model}, {tokenizer}")
    if truncate_dims is not None and truncate_dims <= 0:
        raise ValueError("truncate dimensions must be positive")
    if truncate_dims is not None and projection is not None:
        raise ValueError("truncate-dims cannot be combined with a projection")
    # ONNX external-data exports conventionally share the graph filename as a
    # prefix (for example model.onnx_data). Explicit assets cover other naming.
    assets = {model.resolve()}
    assets.update(
        candidate.resolve()
        for candidate in model.parent.iterdir()
        if candidate.is_file() and candidate.name.startswith(model.name)
    )
    assets.update(path.resolve() for path in extra_model_assets or [])
    if any(not path.is_file() for path in assets):
        raise ValueError("one or more model assets do not exist")
    named_assets = []
    used_names = set()
    for path in sorted(assets, key=lambda value: value.name):
        if path.name in used_names:
            raise ValueError(f"duplicate model asset name {path.name}")
        used_names.add(path.name)
        named_assets.append({"name": path.name, "sha256": sha256_file(path)})
    contract = {
        "schema": SPACE_CONTRACT_SCHEMA,
        "encoder_assets": named_assets,
        "tokenizer_sha256": sha256_file(tokenizer),
        "pooling": pooling,
        "max_length": max_length,
        "query_prefix": query_prefix,
        "corpus_prefix": corpus_prefix,
        "projection_sha256": sha256_file(projection) if projection else None,
        "output_normalization": "l2-f32-v1",
    }
    # Keep the pre-Matryoshka canonical contract byte-for-byte stable when
    # truncation is disabled. In particular, do not serialize a null field.
    if truncate_dims is not None:
        contract["truncate_dims"] = truncate_dims
    if pre_truncate_layer_norm:
        contract["pre_truncate_layer_norm"] = "per-row-epsilon-1e-5-v1"
    if query_adapter is not None:
        if not query_adapter.is_file():
            raise ValueError(f"missing query adapter: {query_adapter}")
        # This field deliberately names the direction. Treating the adapter as
        # a symmetric projection would silently corrupt corpus compatibility.
        contract["query_adapter_sha256"] = sha256_file(query_adapter)
        contract["query_adapter_application"] = "post-encoder-query-only-v1"
    return contract


def contract_space_id(contract: dict) -> str:
    canonical = json.dumps(contract, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


class Encoder:
    def __init__(
        self,
        model: Path,
        tokenizer_path: Path,
        pooling: str,
        max_length: int,
        projection: tuple[np.ndarray, np.ndarray] | None = None,
        truncate_dims: int | None = None,
        pre_truncate_layer_norm: bool = False,
    ) -> None:
        try:
            import onnxruntime as ort
            from tokenizers import Tokenizer
        except ImportError as error:
            raise SystemExit(
                "install build dependencies: python -m pip install numpy onnxruntime tokenizers"
            ) from error

        self.session = ort.InferenceSession(str(model), providers=["CPUExecutionProvider"])
        self.tokenizer = Tokenizer.from_file(str(tokenizer_path))
        self.tokenizer.enable_padding(pad_id=0, pad_token="[PAD]")
        self.tokenizer.enable_truncation(max_length=max_length)
        self.pooling = pooling
        self.projection = projection
        if truncate_dims is not None and truncate_dims <= 0:
            raise ValueError("truncate dimensions must be positive")
        if truncate_dims is not None and projection is not None:
            raise ValueError("truncate-dims cannot be combined with a projection")
        self.truncate_dims = truncate_dims
        self.pre_truncate_layer_norm = pre_truncate_layer_norm
        self.output_names = {output.name for output in self.session.get_outputs()}
        self.input_names = {item.name for item in self.session.get_inputs()}

    def encode(self, texts: list[str], batch_size: int) -> np.ndarray:
        chunks = []
        for chunk in batched(texts, batch_size):
            encoded = self.tokenizer.encode_batch(chunk)
            ids = np.asarray([item.ids for item in encoded], dtype=np.int64)
            mask = np.asarray([item.attention_mask for item in encoded], dtype=np.int64)
            types = np.asarray([item.type_ids for item in encoded], dtype=np.int64)
            inputs = {"input_ids": ids, "attention_mask": mask}
            if "token_type_ids" in self.input_names:
                inputs["token_type_ids"] = types

            if "sentence_embedding" in self.output_names:
                vectors = self.session.run(["sentence_embedding"], inputs)[0]
            else:
                hidden_name = (
                    "last_hidden_state"
                    if "last_hidden_state" in self.output_names
                    else self.session.get_outputs()[0].name
                )
                hidden = self.session.run([hidden_name], inputs)[0]
                pooling = "cls" if self.pooling == "auto" else self.pooling
                if pooling == "cls":
                    vectors = hidden[:, 0, :]
                else:
                    expanded = mask[..., None].astype(np.float32)
                    vectors = (hidden * expanded).sum(axis=1) / np.maximum(
                        expanded.sum(axis=1), 1e-12
                    )

            chunks.append(
                apply_projection(
                    vectors,
                    self.projection,
                    self.truncate_dims,
                    self.pre_truncate_layer_norm,
                )
            )
        if not chunks:
            raise ValueError("input contains no records")
        return np.concatenate(chunks).astype("<f4", copy=False)


def model_paths(args: argparse.Namespace) -> tuple[Path, Path]:
    model_dir = args.model_dir
    return (
        args.model or model_dir / "model_quantized.onnx",
        args.tokenizer or model_dir / "tokenizer.json",
    )


def build_encoder(args: argparse.Namespace) -> Encoder:
    model, tokenizer = model_paths(args)
    if not model.is_file() or not tokenizer.is_file():
        raise ValueError(f"missing model or tokenizer: {model}, {tokenizer}")
    projection = load_projection(args.projection) if args.projection else None
    return Encoder(
        model,
        tokenizer,
        args.pooling,
        args.max_length,
        projection,
        args.truncate_dims,
        args.pre_truncate_layer_norm,
    )


def corpus(args: argparse.Namespace) -> None:
    rows = read_jsonl(args.input)
    canonical_texts = []
    for index, row in enumerate(rows, 1):
        text = row.get("text")
        if not isinstance(text, str) or not text.strip():
            raise ValueError(f"corpus row {index} has no non-empty text")
        canonical_texts.append(text.strip())
    if not args.verse_output and not args.window_output:
        raise ValueError("corpus encoding requires verse-output or window-output")
    encoder = build_encoder(args)
    if args.verse_output:
        texts = [args.corpus_prefix + text for text in canonical_texts]
        encoder.encode(texts, args.batch_size).tofile(args.verse_output)
    if args.window_output:
        windows = [
            args.corpus_prefix + text
            for text in chapter_bounded_windows(rows, canonical_texts)
        ]
        encoder.encode(windows, args.batch_size).tofile(args.window_output)


def encode_query_records(
    rows: list[dict],
    encoder: Encoder,
    query_prefix: str,
    batch_size: int,
    segment_embeddings: bool = False,
    query_adapter: np.ndarray | None = None,
) -> list[dict]:
    ids = []
    prompts = []
    for index, row in enumerate(rows, 1):
        prompt_id, prompt = row.get("id"), row.get("prompt")
        if not isinstance(prompt_id, str) or not isinstance(prompt, str):
            raise ValueError(f"query row {index} requires string id and prompt")
        ids.append(prompt_id)
        prompts.append(prompt)

    plans = (
        [plan_query_segments(prompt) for prompt in prompts]
        if segment_embeddings
        else [[] for _ in prompts]
    )
    segment_texts = [segment for plan in plans for segment in plan]
    texts = [query_prefix + prompt for prompt in prompts]
    texts.extend(query_prefix + segment for segment in segment_texts)
    vectors = apply_query_adapter(encoder.encode(texts, batch_size), query_adapter)
    if len(vectors) != len(texts):
        raise AssertionError("query embedding count does not match encoded text count")
    whole_vectors = vectors[: len(ids)]
    segment_vectors = iter(vectors[len(ids) :])
    records = []
    if len(ids) != len(whole_vectors) or len(ids) != len(plans):
        raise AssertionError("query embedding count does not match query count")
    for prompt_id, prompt, vector, plan in zip(ids, prompts, whole_vectors, plans):
        record = {
            "id": prompt_id,
            PROMPT_UTF8_SHA256_FIELD: prompt_utf8_sha256(prompt),
            "embedding": vector.tolist(),
        }
        if plan:
            record["planner"] = QUERY_SEGMENT_PLANNER
            record["segments"] = [
                {"text": segment, "embedding": next(segment_vectors).tolist()}
                for segment in plan
            ]
        records.append(record)
    return records


def queries(args: argparse.Namespace) -> None:
    rows = read_jsonl(args.input)
    records = encode_query_records(
        rows,
        build_encoder(args),
        args.query_prefix,
        args.batch_size,
        args.segment_embeddings,
        load_query_adapter(args.query_adapter) if args.query_adapter else None,
    )
    with args.output.open("w", encoding="utf-8") as output:
        for record in records:
            output.write(json.dumps(record, separators=(",", ":")) + "\n")


def adapt_query_file(args: argparse.Namespace) -> None:
    """Apply a query-only adapter to previously encoded JSONL records."""

    rows = read_jsonl(args.input)
    matrix = load_query_adapter(args.query_adapter)
    output_rows = []
    for line_number, row in enumerate(rows, 1):
        prompt_id = row.get("id")
        embedding = row.get("embedding")
        if not isinstance(prompt_id, str) or not isinstance(embedding, list):
            raise ValueError(f"query row {line_number} requires id and embedding")
        prompt_digest = validate_prompt_utf8_sha256(
            row.get(PROMPT_UTF8_SHA256_FIELD), line_number
        )
        whole = apply_query_adapter(
            np.asarray([embedding], dtype=np.float32), matrix
        )[0]
        transformed = dict(row)
        # The transform changes only the vector space.  Preserve the exact
        # prompt binding byte-for-byte so the evaluator can reject stale or
        # swapped records before search.
        transformed[PROMPT_UTF8_SHA256_FIELD] = prompt_digest
        transformed["embedding"] = whole.tolist()
        segments = row.get("segments", [])
        if segments:
            if row.get("planner") != QUERY_SEGMENT_PLANNER or not isinstance(segments, list):
                raise ValueError(f"query row {line_number} has invalid segment metadata")
            adapted_segments = []
            for segment in segments:
                if not isinstance(segment, dict) or not isinstance(
                    segment.get("embedding"), list
                ):
                    raise ValueError(f"query row {line_number} has an invalid segment")
                vector = apply_query_adapter(
                    np.asarray([segment["embedding"]], dtype=np.float32), matrix
                )[0]
                adapted_segment = dict(segment)
                adapted_segment["embedding"] = vector.tolist()
                adapted_segments.append(adapted_segment)
            transformed["segments"] = adapted_segments
        output_rows.append(transformed)
    with args.output.open("w", encoding="utf-8") as output:
        for row in output_rows:
            output.write(json.dumps(row, separators=(",", ":")) + "\n")


def fit_projection(args: argparse.Namespace) -> None:
    matrices = [read_f32_matrix(path, args.input_dims) for path in args.input]
    # Encoder outputs and runtime queries are normalized before projection, so
    # fit in that identical space even when a caller supplies raw f32 rows.
    vectors = normalize_vectors(np.concatenate(matrices))
    mean, components = fit_pca_projection(vectors, args.output_dims)
    save_projection(args.output, mean, components)
    print(
        f"wrote deterministic PCA projection {args.input_dims}D -> "
        f"{args.output_dims}D ({args.output})"
    )


def project_file(args: argparse.Namespace) -> None:
    vectors = read_f32_matrix(args.input, args.input_dims)
    projected = apply_projection(vectors, load_projection(args.projection))
    projected.tofile(args.output)


def print_space_id(args: argparse.Namespace) -> None:
    model, tokenizer = model_paths(args)
    contract = embedding_space_contract(
        model=model,
        tokenizer=tokenizer,
        pooling=args.pooling,
        max_length=args.max_length,
        query_prefix=args.query_prefix,
        corpus_prefix=args.corpus_prefix,
        projection=args.projection,
        extra_model_assets=args.model_asset,
        truncate_dims=args.truncate_dims,
        query_adapter=args.query_adapter,
        pre_truncate_layer_norm=args.pre_truncate_layer_norm,
    )
    result = {
        "embedding_space_id": contract_space_id(contract),
        "contract": contract,
    }
    rendered = json.dumps(result, sort_keys=True, separators=(",", ":"))
    if args.output:
        args.output.write_text(rendered + "\n", encoding="utf-8")
    else:
        print(rendered)


def common(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--model", type=Path)
    parser.add_argument("--tokenizer", type=Path)
    parser.add_argument("--pooling", choices=["auto", "cls", "mean"], default="auto")
    parser.add_argument("--max-length", type=int, default=128)
    parser.add_argument("--batch-size", type=int, default=64)
    parser.add_argument(
        "--projection",
        type=Path,
        help="deterministic VRPROJ01 projection applied after encoder normalization",
    )
    parser.add_argument(
        "--truncate-dims",
        type=int,
        help=(
            "retain the first N normalized encoder dimensions, then renormalize "
            "(Matryoshka encoders only; cannot be combined with --projection)"
        ),
    )
    parser.add_argument(
        "--pre-truncate-layer-norm",
        action="store_true",
        help=(
            "apply deterministic per-row layer normalization (epsilon 1e-5) "
            "before Matryoshka truncation and L2 normalization"
        ),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    corpus_parser = commands.add_parser("corpus")
    common(corpus_parser)
    corpus_parser.add_argument("--input", type=Path, required=True)
    corpus_parser.add_argument(
        "--verse-output",
        type=Path,
        help="optional row-major verse vectors (omit to build only windows)",
    )
    corpus_parser.add_argument("--window-output", type=Path)
    corpus_parser.add_argument("--corpus-prefix", default="")
    corpus_parser.set_defaults(run=corpus)
    query_parser = commands.add_parser("queries")
    common(query_parser)
    query_parser.add_argument("--input", type=Path, required=True)
    query_parser.add_argument("--output", type=Path, required=True)
    query_parser.add_argument("--query-prefix", default="")
    query_parser.add_argument(
        "--query-adapter",
        type=Path,
        help="VRQADP01 transform applied after query encoding only",
    )
    query_parser.add_argument(
        "--segment-embeddings",
        action="store_true",
        help=(
            "also encode 2-4 conservative query-only clause segments and emit "
            "verse-recall-clause-v2 JSONL metadata"
        ),
    )
    query_parser.set_defaults(run=queries)
    adapt_parser = commands.add_parser("adapt-queries")
    adapt_parser.add_argument("--input", type=Path, required=True)
    adapt_parser.add_argument("--output", type=Path, required=True)
    adapt_parser.add_argument("--query-adapter", type=Path, required=True)
    adapt_parser.set_defaults(run=adapt_query_file)
    fit_parser = commands.add_parser("fit-projection")
    fit_parser.add_argument("--input", type=Path, action="append", required=True)
    fit_parser.add_argument("--input-dims", type=int, required=True)
    fit_parser.add_argument("--output-dims", type=int, required=True)
    fit_parser.add_argument("--output", type=Path, required=True)
    fit_parser.set_defaults(run=fit_projection)
    project_parser = commands.add_parser("project")
    project_parser.add_argument("--input", type=Path, required=True)
    project_parser.add_argument("--input-dims", type=int, required=True)
    project_parser.add_argument("--projection", type=Path, required=True)
    project_parser.add_argument("--output", type=Path, required=True)
    project_parser.set_defaults(run=project_file)
    space_parser = commands.add_parser("space-id")
    common(space_parser)
    space_parser.add_argument("--query-prefix", default="")
    space_parser.add_argument("--corpus-prefix", default="")
    space_parser.add_argument("--model-asset", type=Path, action="append", default=[])
    space_parser.add_argument(
        "--query-adapter",
        type=Path,
        help="VRQADP01 query-only transform included in the space identity",
    )
    space_parser.add_argument("--output", type=Path)
    space_parser.set_defaults(run=print_space_id)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if getattr(args, "max_length", 1) <= 0 or getattr(args, "batch_size", 1) <= 0:
        raise ValueError("max-length and batch-size must be positive")
    truncate_dims = getattr(args, "truncate_dims", None)
    if truncate_dims is not None and truncate_dims <= 0:
        raise ValueError("truncate dimensions must be positive")
    if truncate_dims is not None and getattr(args, "projection", None) is not None:
        raise ValueError("truncate-dims cannot be combined with a projection")
    args.run(args)


if __name__ == "__main__":
    main()
