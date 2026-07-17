#!/usr/bin/env python3
"""Audit, extract, and validate the Study Bible verse embedding space.

This is an experimental, corpus-only alignment pipeline.  It never reads an
evaluation prompt or a gold reference.  Its supervision consists solely of
the canonical coordinate shared by a BSB verse embedding and the independently
published Study Bible ``verse_vectors`` row for that coordinate.

The Study Bible database uses sqlite-vec shadow tables.  Reading the virtual
table normally requires loading a native extension; the extractor instead
validates and reads those documented shadow rows directly.  Split Greek/Hebrew
rows are combined at their effective English coordinate before being aligned
to the 31,086-row BSB corpus.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sqlite3
import tempfile
import time
from pathlib import Path
from typing import Iterator

import numpy as np


STUDY_BOOK_CODES = (
    "Gen", "Exo", "Lev", "Num", "Deu", "Jos", "Jdg", "Rut", "1Sa", "2Sa",
    "1Ki", "2Ki", "1Ch", "2Ch", "Ezr", "Neh", "Est", "Job", "Psa", "Pro",
    "Ecc", "Sng", "Isa", "Jer", "Lam", "Ezk", "Dan", "Hos", "Jol", "Amo",
    "Oba", "Jon", "Mic", "Nam", "Hab", "Zep", "Hag", "Zec", "Mal", "Mat",
    "Mrk", "Luk", "Jhn", "Act", "Rom", "1Co", "2Co", "Gal", "Eph", "Php",
    "Col", "1Th", "2Th", "1Ti", "2Ti", "Tit", "Phm", "Heb", "Jas", "1Pe",
    "2Pe", "1Jn", "2Jn", "3Jn", "Jud", "Rev",
)

DISPLAY_REF = re.compile(
    r"^(?P<book>.+?) (?P<chapter>[1-9][0-9]*):(?P<verse>[1-9][0-9]*)$"
)
ENGLISH_SPLIT_REF = re.compile(r"\[(?P<chapter>[1-9][0-9]*)\.(?P<verse>[1-9][0-9]*)\]")
ALIGNMENT_SEED = "study-align-v1"

# These are declared before corpus validation.  They are intentionally about
# held-out books, not a product evaluation suite.
VIABILITY_THRESHOLDS = {
    "global_candidate_recall_at_1": 0.50,
    "global_candidate_recall_at_10": 0.85,
    "within_book_recall_at_10": 0.90,
    "paired_vs_negative_median_margin": 0.15,
    "cross_space_neighbor_recall_at_10": 0.15,
}


def sha256_file(path: Path, block_size: int = 8 * 1024 * 1024) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while block := handle.read(block_size):
            digest.update(block)
    return digest.hexdigest()


def canonical_json(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def read_canonical(path: Path) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    seen: set[str] = set()
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        reference = value.get("ref") if isinstance(value, dict) else None
        match = DISPLAY_REF.fullmatch(reference or "")
        if match is None:
            raise ValueError(f"{path}:{line_number}: invalid canonical reference")
        if reference in seen:
            raise ValueError(f"{path}:{line_number}: duplicate reference {reference}")
        seen.add(reference)
        rows.append(
            {
                "ref": reference,
                "book": match.group("book"),
                "chapter": match.group("chapter"),
                "verse": match.group("verse"),
            }
        )
    if not rows:
        raise ValueError(f"{path}: canonical input is empty")
    return rows


def canonical_book_map(rows: list[dict[str, str]]) -> dict[str, str]:
    books: list[str] = []
    for row in rows:
        if not books or books[-1] != row["book"]:
            if row["book"] in books:
                raise ValueError(f"canonical book {row['book']} is not contiguous")
            books.append(row["book"])
    if len(books) > len(STUDY_BOOK_CODES):
        raise ValueError("canonical input has more books than the Study Bible schema")
    return dict(zip(STUDY_BOOK_CODES[: len(books)], books))


def effective_source_coordinate(
    raw_reference: str,
    book_code: str,
    chapter: int,
    verse: int,
) -> tuple[str, int, int] | None:
    """Return the source coordinate in common English versification.

    STEPBible rows sometimes split a source-language verse at an English verse
    boundary.  In references such as ``2Co.13.13[13.14]``, the bracketed value
    is the effective English coordinate of that fragment.  Parenthesized
    values represent a different versification and deliberately do not
    override the leading coordinate.
    """

    split = ENGLISH_SPLIT_REF.search(raw_reference or "")
    if split is not None:
        chapter = int(split.group("chapter"))
        verse = int(split.group("verse"))
    if chapter <= 0 or verse <= 0:
        return None
    return book_code, chapter, verse


def database_uri(path: Path) -> str:
    return f"file:{path.resolve()}?mode=ro"


def table_names(connection: sqlite3.Connection) -> set[str]:
    return {
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type IN ('table','view')"
        )
    }


def source_reference_map(
    connection: sqlite3.Connection,
    canonical_rows: list[dict[str, str]],
) -> tuple[dict[int, int], dict[str, object]]:
    code_to_book = canonical_book_map(canonical_rows)
    canonical_by_coordinate = {
        (row["book"], int(row["chapter"]), int(row["verse"])): index
        for index, row in enumerate(canonical_rows)
    }

    source_to_canonical: dict[int, int] = {}
    matched_fragments: dict[int, list[int]] = {}
    excluded: list[str] = []
    for source_id, raw_reference, code, chapter, verse in connection.execute(
        "SELECT id, reference, book, chapter, verse FROM verses ORDER BY id"
    ):
        book = code_to_book.get(code)
        if book is None:
            excluded.append(raw_reference)
            continue
        source_coordinate = effective_source_coordinate(
            raw_reference, code, int(chapter), int(verse)
        )
        if source_coordinate is None:
            excluded.append(raw_reference)
            continue
        _, effective_chapter, effective_verse = source_coordinate
        canonical_index = canonical_by_coordinate.get(
            (book, effective_chapter, effective_verse)
        )
        if canonical_index is None:
            excluded.append(f"{book} {effective_chapter}:{effective_verse}")
            continue
        source_id = int(source_id)
        source_to_canonical[source_id] = canonical_index
        matched_fragments.setdefault(canonical_index, []).append(source_id)

    missing = [
        row["ref"] for index, row in enumerate(canonical_rows) if index not in matched_fragments
    ]
    duplicate_references = [
        {
            "ref": canonical_rows[index]["ref"],
            "source_fragment_count": len(source_ids),
        }
        for index, source_ids in sorted(matched_fragments.items())
        if len(source_ids) > 1
    ]
    audit = {
        "canonical_rows": len(canonical_rows),
        "canonical_books": len(code_to_book),
        "matched_canonical_rows": len(matched_fragments),
        "matched_source_fragments": len(source_to_canonical),
        "coverage": len(matched_fragments) / len(canonical_rows),
        "missing_canonical_references": missing,
        "duplicate_fragment_coordinates": duplicate_references,
        "excluded_source_reference_count": len(excluded),
        "excluded_source_references": sorted(set(excluded)),
    }
    return source_to_canonical, audit


def shadow_chunks(
    connection: sqlite3.Connection,
    dims: int,
) -> Iterator[tuple[int, np.ndarray, np.ndarray]]:
    required = {
        "verse_vectors_chunks",
        "verse_vectors_rowids",
        "verse_vectors_vector_chunks00",
    }
    missing = sorted(required - table_names(connection))
    if missing:
        raise ValueError(f"Study database is missing sqlite-vec shadow tables: {missing}")

    shadow_index = {
        (int(chunk_id), int(chunk_offset)): int(source_id)
        for source_id, chunk_id, chunk_offset in connection.execute(
            "SELECT rowid, chunk_id, chunk_offset FROM verse_vectors_rowids ORDER BY rowid"
        )
    }
    observed_slots: set[tuple[int, int]] = set()
    active_count = 0
    query = """
        SELECT chunks.chunk_id, chunks.size, chunks.validity, chunks.rowids, vectors.vectors
        FROM verse_vectors_chunks AS chunks
        JOIN verse_vectors_vector_chunks00 AS vectors
          ON vectors.rowid = chunks.chunk_id
        ORDER BY chunks.chunk_id
    """
    for chunk_id, size, validity_blob, rowids_blob, vectors_blob in connection.execute(query):
        chunk_id, size = int(chunk_id), int(size)
        if size <= 0:
            raise ValueError(f"sqlite-vec chunk {chunk_id} has invalid size {size}")
        if len(validity_blob) != math.ceil(size / 8):
            raise ValueError(f"sqlite-vec chunk {chunk_id} has invalid validity bitmap")
        if len(rowids_blob) != size * 8:
            raise ValueError(f"sqlite-vec chunk {chunk_id} has invalid rowid payload")
        if len(vectors_blob) != size * dims * 4:
            raise ValueError(f"sqlite-vec chunk {chunk_id} has invalid vector payload")

        rowids = np.frombuffer(rowids_blob, dtype="<i8", count=size)
        vectors = np.frombuffer(vectors_blob, dtype="<f4", count=size * dims).reshape(
            size, dims
        )
        active_slots = np.fromiter(
            (
                slot
                for slot in range(size)
                if validity_blob[slot // 8] & (1 << (slot % 8))
            ),
            dtype=np.int64,
        )
        active_rowids = rowids[active_slots]
        for slot, source_id in zip(active_slots.tolist(), active_rowids.tolist()):
            key = (chunk_id, slot)
            if key in observed_slots:
                raise ValueError(f"sqlite-vec shadow slot {key} is duplicated")
            observed_slots.add(key)
            if shadow_index.get(key) != source_id:
                raise ValueError(f"sqlite-vec rowid index disagrees at slot {key}")
        active_count += len(active_slots)
        yield chunk_id, active_rowids, vectors[active_slots]

    if active_count != len(shadow_index) or observed_slots != set(shadow_index):
        raise ValueError(
            "sqlite-vec validity bitmap and rowid index have different active rows"
        )


def write_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def extract(
    database: Path,
    canonical: Path,
    output: Path,
    manifest_output: Path,
    dims: int = 1536,
    expected_database_sha256: str | None = None,
) -> dict[str, object]:
    if dims <= 0:
        raise ValueError("vector dimensions must be positive")
    database_hash = sha256_file(database)
    if (
        expected_database_sha256 is not None
        and database_hash != expected_database_sha256.lower()
    ):
        raise ValueError(
            f"Study database SHA-256 is {database_hash}, expected "
            f"{expected_database_sha256.lower()}"
        )
    canonical_rows = read_canonical(canonical)
    output.parent.mkdir(parents=True, exist_ok=True)
    manifest_output.parent.mkdir(parents=True, exist_ok=True)

    uri = database_uri(database)
    with sqlite3.connect(uri, uri=True) as connection:
        names = table_names(connection)
        if "verses" not in names:
            raise ValueError(f"{database}: missing verses table")
        source_to_canonical, reference_audit = source_reference_map(
            connection, canonical_rows
        )
        if reference_audit["missing_canonical_references"]:
            raise ValueError(
                "Study vectors do not cover every canonical coordinate: "
                f"{reference_audit['missing_canonical_references'][:8]}"
            )

        counts = np.zeros(len(canonical_rows), dtype=np.uint16)
        with tempfile.NamedTemporaryFile(
            prefix=f".{output.name}.", suffix=".tmp", dir=output.parent, delete=False
        ) as temporary:
            temporary_path = Path(temporary.name)
        try:
            aligned = np.memmap(
                temporary_path,
                mode="w+",
                dtype="<f4",
                shape=(len(canonical_rows), dims),
            )
            aligned[:] = 0.0
            active_vectors = 0
            mapped_vectors = 0
            invalid_norms = 0
            chunk_count = 0
            for _, source_ids, vectors in shadow_chunks(connection, dims):
                chunk_count += 1
                active_vectors += len(source_ids)
                norms = np.linalg.norm(vectors.astype(np.float64), axis=1)
                finite = np.isfinite(norms) & (norms > 0.0)
                invalid_norms += int((~finite).sum())
                if not bool(finite.all()):
                    raise ValueError("Study database contains a non-finite or zero vector")
                normalized = vectors / norms[:, None]
                for source_id, vector in zip(source_ids.tolist(), normalized):
                    canonical_index = source_to_canonical.get(int(source_id))
                    if canonical_index is None:
                        continue
                    aligned[canonical_index] += vector.astype(np.float32, copy=False)
                    counts[canonical_index] += 1
                    mapped_vectors += 1

            if np.any(counts == 0):
                missing = np.flatnonzero(counts == 0)[:8].tolist()
                raise ValueError(f"no vector payload for canonical indices {missing}")
            for start in range(0, len(canonical_rows), 1024):
                stop = min(start + 1024, len(canonical_rows))
                block = np.asarray(aligned[start:stop], dtype=np.float64)
                norms = np.linalg.norm(block, axis=1)
                if np.any(~np.isfinite(norms)) or np.any(norms <= 0.0):
                    raise ValueError("aggregated Study vector has an invalid norm")
                aligned[start:stop] = (block / norms[:, None]).astype(np.float32)
            aligned.flush()
            del aligned
            temporary_path.replace(output)
        except BaseException:
            temporary_path.unlink(missing_ok=True)
            raise

        version = None
        if "verse_vectors_info" in names:
            version = {
                str(key): value
                for key, value in connection.execute(
                    "SELECT key, value FROM verse_vectors_info ORDER BY key"
                )
            }
        source_verse_rows = int(connection.execute("SELECT COUNT(*) FROM verses").fetchone()[0])

    duplicate_count = int(np.sum(counts > 1))
    manifest: dict[str, object] = {
        "format": "verse-recall.study-vector-extract.v1",
        "pipeline_sha256": sha256_file(Path(__file__)),
        "database": str(database.resolve()),
        "database_sha256": database_hash,
        "canonical": str(canonical.resolve()),
        "canonical_sha256": sha256_file(canonical),
        "output": str(output.resolve()),
        "output_sha256": sha256_file(output),
        "output_bytes": output.stat().st_size,
        "dtype": "little-endian-f32",
        "dimensions": dims,
        "source_verse_rows": source_verse_rows,
        "source_active_vectors": active_vectors,
        "source_mapped_vectors": mapped_vectors,
        "sqlite_vec_chunk_count": chunk_count,
        "sqlite_vec_version": version,
        "invalid_vector_norms": invalid_norms,
        "aggregated_canonical_coordinates": duplicate_count,
        "reference_audit": reference_audit,
        "normalization": "l2(each-source-fragment); mean-via-sum; l2(canonical-row)",
    }
    write_json(manifest_output, manifest)
    return manifest


def load_matrix(path: Path, rows: int, dims: int) -> np.memmap:
    expected = rows * dims * 4
    if path.stat().st_size != expected:
        raise ValueError(f"{path} has {path.stat().st_size} bytes; expected {expected}")
    return np.memmap(path, mode="r", dtype="<f4", shape=(rows, dims))


def normalized_matrix(matrix: np.ndarray) -> np.ndarray:
    values = np.asarray(matrix, dtype=np.float32)
    norms = np.linalg.norm(values.astype(np.float64), axis=1)
    if np.any(~np.isfinite(norms)) or np.any(norms <= 0.0):
        raise ValueError("alignment input contains a non-finite or zero row")
    return (values / norms[:, None]).astype(np.float32)


def stable_fold(book: str, fold_count: int) -> int:
    digest = hashlib.sha256(f"{ALIGNMENT_SEED}:{book}".encode("utf-8")).digest()
    return int.from_bytes(digest[:8], "big") % fold_count


def stable_sample(indices: np.ndarray, references: list[str], limit: int) -> np.ndarray:
    if len(indices) <= limit:
        return indices.copy()
    ranked = sorted(
        indices.tolist(),
        key=lambda index: (
            hashlib.sha256(
                f"{ALIGNMENT_SEED}:sample:{references[index]}".encode("utf-8")
            ).digest(),
            index,
        ),
    )
    return np.asarray(sorted(ranked[:limit]), dtype=np.int64)


def fit_rectangular_procrustes(
    source: np.ndarray,
    target: np.ndarray,
    indices: np.ndarray,
    batch_size: int = 2048,
) -> tuple[np.ndarray, np.ndarray]:
    if source.shape[0] != target.shape[0]:
        raise ValueError("paired alignment matrices have different row counts")
    if len(indices) < max(source.shape[1], 2):
        raise ValueError("not enough corpus pairs to fit the transform")
    cross = np.zeros((source.shape[1], target.shape[1]), dtype=np.float64)
    for start in range(0, len(indices), batch_size):
        selected = indices[start : start + batch_size]
        x = normalized_matrix(source[selected]).astype(np.float64)
        y = normalized_matrix(target[selected]).astype(np.float64)
        cross += x.T @ y
    u, singular_values, vt = np.linalg.svd(cross, full_matrices=False)
    transform = (u @ vt).astype(np.float32)
    return transform, singular_values


def ranks_from_scores(scores: np.ndarray, target_columns: np.ndarray) -> np.ndarray:
    target_scores = scores[np.arange(len(scores)), target_columns]
    # Strictly larger scores outrank the target.  Candidate order is the
    # deterministic tie-break, matching canonical-order retrieval behavior.
    higher = np.sum(scores > target_scores[:, None], axis=1)
    ties_before = np.sum(
        (scores == target_scores[:, None])
        & (np.arange(scores.shape[1])[None, :] < target_columns[:, None]),
        axis=1,
    )
    return higher + ties_before + 1


def retrieval_metrics(ranks: np.ndarray) -> dict[str, float]:
    return {
        "queries": int(len(ranks)),
        "recall_at_1": float(np.mean(ranks <= 1)),
        "recall_at_3": float(np.mean(ranks <= 3)),
        "recall_at_10": float(np.mean(ranks <= 10)),
        "mrr": float(np.mean(1.0 / ranks)),
        "median_rank": float(np.median(ranks)),
    }


def transformed_queries(source: np.ndarray, transform: np.ndarray) -> np.ndarray:
    values = normalized_matrix(source) @ transform
    return normalized_matrix(values)


def validate_transform(
    source: np.ndarray,
    target: np.ndarray,
    transform: np.ndarray,
    canonical_rows: list[dict[str, str]],
    holdout_indices: np.ndarray,
    queries_per_book: int,
    global_candidates: int,
) -> dict[str, object]:
    references = [row["ref"] for row in canonical_rows]
    books = [row["book"] for row in canonical_rows]
    holdout_books = sorted({books[index] for index in holdout_indices.tolist()})
    by_book = {
        book: np.asarray(
            [index for index in holdout_indices.tolist() if books[index] == book],
            dtype=np.int64,
        )
        for book in holdout_books
    }
    query_indices = np.concatenate(
        [stable_sample(by_book[book], references, queries_per_book) for book in holdout_books]
    )
    query_indices.sort()
    transformed = transformed_queries(source[query_indices], transform)
    normalized_target = target

    within_ranks: list[int] = []
    query_position = {index: position for position, index in enumerate(query_indices.tolist())}
    for book in holdout_books:
        candidates = by_book[book]
        queries = stable_sample(candidates, references, queries_per_book)
        q = transformed[[query_position[index] for index in queries.tolist()]]
        y = normalized_matrix(normalized_target[candidates])
        scores = q @ y.T
        candidate_position = {index: pos for pos, index in enumerate(candidates.tolist())}
        target_columns = np.asarray(
            [candidate_position[index] for index in queries.tolist()], dtype=np.int64
        )
        within_ranks.extend(ranks_from_scores(scores, target_columns).tolist())

    required = set(query_indices.tolist())
    ranked_candidates = sorted(
        range(source.shape[0]),
        key=lambda index: (
            hashlib.sha256(
                f"{ALIGNMENT_SEED}:candidate:{references[index]}".encode("utf-8")
            ).digest(),
            index,
        ),
    )
    candidate_set = set(required)
    for index in ranked_candidates:
        if len(candidate_set) >= max(global_candidates, len(required)):
            break
        candidate_set.add(index)
    candidate_indices = np.asarray(sorted(candidate_set), dtype=np.int64)
    candidate_position = {
        index: position for position, index in enumerate(candidate_indices.tolist())
    }
    global_ranks: list[int] = []
    y_candidates = normalized_matrix(normalized_target[candidate_indices])
    for start in range(0, len(query_indices), 32):
        stop = min(start + 32, len(query_indices))
        scores = transformed[start:stop] @ y_candidates.T
        target_columns = np.asarray(
            [candidate_position[index] for index in query_indices[start:stop].tolist()],
            dtype=np.int64,
        )
        global_ranks.extend(ranks_from_scores(scores, target_columns).tolist())

    pair_scores = np.sum(
        transformed * normalized_matrix(normalized_target[query_indices]), axis=1
    )
    negative_indices = []
    for index in query_indices.tolist():
        digest = hashlib.sha256(
            f"{ALIGNMENT_SEED}:negative:{references[index]}".encode("utf-8")
        ).digest()
        negative = int.from_bytes(digest[:8], "big") % source.shape[0]
        if negative == index:
            negative = (negative + 1) % source.shape[0]
        negative_indices.append(negative)
    negative_scores = np.sum(
        transformed * normalized_matrix(normalized_target[np.asarray(negative_indices)]),
        axis=1,
    )

    neighbor_recalls: list[float] = []
    for book in holdout_books:
        sample = stable_sample(by_book[book], references, 128)
        if len(sample) < 12:
            continue
        x = normalized_matrix(source[sample])
        y = normalized_matrix(target[sample])
        x_scores = x @ x.T
        y_scores = y @ y.T
        np.fill_diagonal(x_scores, -np.inf)
        np.fill_diagonal(y_scores, -np.inf)
        x_neighbors = np.argpartition(-x_scores, kth=9, axis=1)[:, :10]
        y_neighbors = np.argpartition(-y_scores, kth=9, axis=1)[:, :10]
        for left, right in zip(x_neighbors, y_neighbors):
            neighbor_recalls.append(len(set(left.tolist()) & set(right.tolist())) / 10.0)

    within = retrieval_metrics(np.asarray(within_ranks, dtype=np.int64))
    global_metrics = retrieval_metrics(np.asarray(global_ranks, dtype=np.int64))
    paired_median = float(np.median(pair_scores))
    negative_median = float(np.median(negative_scores))
    neighbor_recall = float(np.mean(neighbor_recalls))
    metrics = {
        "holdout_books": holdout_books,
        "holdout_rows": int(len(holdout_indices)),
        "sampled_queries": int(len(query_indices)),
        "within_book": within,
        "global_candidate_pool": {
            **global_metrics,
            "candidate_rows": int(len(candidate_indices)),
        },
        "paired_cosine_median": paired_median,
        "negative_cosine_median": negative_median,
        "paired_vs_negative_median_margin": paired_median - negative_median,
        "cross_space_neighbor_recall_at_10": neighbor_recall,
        "cross_space_neighbor_rows": len(neighbor_recalls),
    }
    gate_values = {
        "global_candidate_recall_at_1": global_metrics["recall_at_1"],
        "global_candidate_recall_at_10": global_metrics["recall_at_10"],
        "within_book_recall_at_10": within["recall_at_10"],
        "paired_vs_negative_median_margin": metrics[
            "paired_vs_negative_median_margin"
        ],
        "cross_space_neighbor_recall_at_10": neighbor_recall,
    }
    gate = {
        name: {
            "value": gate_values[name],
            "threshold": threshold,
            "passed": gate_values[name] >= threshold,
        }
        for name, threshold in VIABILITY_THRESHOLDS.items()
    }
    metrics["viability_gate"] = gate
    metrics["viable"] = all(value["passed"] for value in gate.values())
    return metrics


def benchmark_transform(transform: np.ndarray, repeats: int = 200) -> dict[str, float]:
    query = np.linspace(-1.0, 1.0, transform.shape[0], dtype=np.float32)
    query /= np.linalg.norm(query)
    for _ in range(10):
        _ = query @ transform
    samples = []
    for _ in range(repeats):
        started = time.perf_counter_ns()
        _ = query @ transform
        samples.append((time.perf_counter_ns() - started) / 1_000_000.0)
    return {
        "repeats": repeats,
        "median_ms": float(np.median(samples)),
        "p95_ms": float(np.percentile(samples, 95)),
    }


def save_transform(path: Path, transform: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent, delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)
    try:
        np.asarray(transform, dtype="<f4").tofile(temporary_path)
        temporary_path.replace(path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def contract_for_transform(
    transform_path: Path,
    source_dims: int,
    target_dims: int,
    arctic_vectors: Path,
    study_vectors: Path,
    canonical: Path,
    arctic_space_contract: Path,
    study_manifest: Path,
    training: dict[str, object],
) -> dict[str, object]:
    upstream_contract = json.loads(arctic_space_contract.read_text(encoding="utf-8"))
    upstream_space_id = upstream_contract.get("embedding_space_id")
    if not isinstance(upstream_space_id, str) or len(upstream_space_id) != 64:
        raise ValueError("Arctic space contract has no valid embedding_space_id")
    source_manifest = json.loads(study_manifest.read_text(encoding="utf-8"))
    contract: dict[str, object] = {
        "format": "verse-recall.study-vector-alignment.v1",
        "pipeline_sha256": sha256_file(Path(__file__)),
        "algorithm": "uncentered-l2 rectangular orthogonal Procrustes",
        "query_input": "Arctic-M query embedding, l2-normalized",
        "runtime_query_transform": "l2(q) @ W; l2(result); compare to l2 Study vectors",
        "source_dimensions": source_dims,
        "target_dimensions": target_dims,
        "transform_dtype": "little-endian-f32-row-major",
        "transform_sha256": sha256_file(transform_path),
        "transform_bytes": transform_path.stat().st_size,
        "upstream_arctic_embedding_space_id": upstream_space_id,
        "upstream_arctic_contract_sha256": sha256_file(arctic_space_contract),
        "arctic_vectors_sha256": sha256_file(arctic_vectors),
        "study_vectors_sha256": sha256_file(study_vectors),
        "study_extract_manifest_sha256": sha256_file(study_manifest),
        "study_database_sha256": source_manifest.get("database_sha256"),
        "canonical_sha256": sha256_file(canonical),
        "training": training,
        "portable_artifact_note": (
            "The f32 transform adds source_dimensions*target_dimensions*4 bytes. "
            "A full int8 Study corpus would add canonical_rows*target_dimensions "
            "bytes; neither is integrated until the sealed product evaluation gate passes."
        ),
    }
    contract["contract_id"] = hashlib.sha256(canonical_json(contract)).hexdigest()
    return contract


def validate(
    canonical: Path,
    arctic_vectors: Path,
    study_vectors: Path,
    arctic_space_contract: Path,
    study_manifest: Path,
    source_dims: int,
    target_dims: int,
    fold_count: int,
    holdout_fold: int,
    queries_per_book: int,
    global_candidates: int,
    report_output: Path,
    transform_output: Path | None,
    contract_output: Path | None,
    save_failed_transform: bool,
) -> dict[str, object]:
    if fold_count < 2 or not 0 <= holdout_fold < fold_count:
        raise ValueError("holdout fold must be within a fold count of at least two")
    canonical_rows = read_canonical(canonical)
    upstream_contract = json.loads(arctic_space_contract.read_text(encoding="utf-8"))
    upstream_space_id = upstream_contract.get("embedding_space_id")
    if not isinstance(upstream_space_id, str) or len(upstream_space_id) != 64:
        raise ValueError("Arctic space contract has no valid embedding_space_id")
    extract_manifest = json.loads(study_manifest.read_text(encoding="utf-8"))
    actual_study_hash = sha256_file(study_vectors)
    if extract_manifest.get("output_sha256") != actual_study_hash:
        raise ValueError("Study vector file does not match its extraction manifest")
    if extract_manifest.get("canonical_sha256") != sha256_file(canonical):
        raise ValueError("Study extraction manifest names a different canonical corpus")
    source = load_matrix(arctic_vectors, len(canonical_rows), source_dims)
    target = load_matrix(study_vectors, len(canonical_rows), target_dims)
    books = [row["book"] for row in canonical_rows]
    holdout_indices = np.asarray(
        [
            index
            for index, book in enumerate(books)
            if stable_fold(book, fold_count) == holdout_fold
        ],
        dtype=np.int64,
    )
    train_indices = np.asarray(
        [
            index
            for index, book in enumerate(books)
            if stable_fold(book, fold_count) != holdout_fold
        ],
        dtype=np.int64,
    )
    if len(holdout_indices) == 0 or len(train_indices) == 0:
        raise ValueError("deterministic book split produced an empty partition")

    started = time.perf_counter()
    transform, singular_values = fit_rectangular_procrustes(
        source, target, train_indices
    )
    fit_seconds = time.perf_counter() - started
    metrics = validate_transform(
        source,
        target,
        transform,
        canonical_rows,
        holdout_indices,
        queries_per_book,
        global_candidates,
    )
    report: dict[str, object] = {
        "format": "verse-recall.study-vector-book-holdout-validation.v1",
        "pipeline_sha256": sha256_file(Path(__file__)),
        "supervision": "canonical corpus coordinate pairs only; no evaluation prompts or gold",
        "canonical_rows": len(canonical_rows),
        "source_dimensions": source_dims,
        "target_dimensions": target_dims,
        "fold_count": fold_count,
        "holdout_fold": holdout_fold,
        "book_split": {
            "algorithm": "uint64_be(sha256(seed + ':' + book)[:8]) modulo fold_count",
            "seed": ALIGNMENT_SEED,
        },
        "train_rows": int(len(train_indices)),
        "fit_seconds": fit_seconds,
        "singular_value_summary": {
            "largest": float(singular_values[0]),
            "median": float(np.median(singular_values)),
            "smallest": float(singular_values[-1]),
        },
        "orthogonality_max_abs_error": float(
            np.max(np.abs(transform @ transform.T - np.eye(source_dims)))
        ),
        "runtime_query_transform_latency": benchmark_transform(transform),
        "metrics": metrics,
        "pre_registered_thresholds": VIABILITY_THRESHOLDS,
        "arctic_vectors_sha256": sha256_file(arctic_vectors),
        "arctic_space_contract_sha256": sha256_file(arctic_space_contract),
        "upstream_arctic_embedding_space_id": upstream_space_id,
        "study_vectors_sha256": actual_study_hash,
        "study_extract_manifest_sha256": sha256_file(study_manifest),
        "study_database_sha256": extract_manifest.get("database_sha256"),
        "canonical_sha256": sha256_file(canonical),
    }
    viable = bool(metrics["viable"])
    if transform_output is not None and (viable or save_failed_transform):
        save_transform(transform_output, transform)
        report["transform"] = str(transform_output.resolve())
        report["transform_sha256"] = sha256_file(transform_output)
        report["transform_bytes"] = transform_output.stat().st_size
        if contract_output is not None:
            contract = contract_for_transform(
                transform_output,
                source_dims,
                target_dims,
                arctic_vectors,
                study_vectors,
                canonical,
                arctic_space_contract,
                study_manifest,
                {
                    "mode": "book-heldout-validation",
                    "fold_count": fold_count,
                    "holdout_fold": holdout_fold,
                    "train_rows": int(len(train_indices)),
                    "holdout_books": metrics["holdout_books"],
                },
            )
            write_json(contract_output, contract)
            report["contract"] = str(contract_output.resolve())
            report["contract_id"] = contract["contract_id"]
    report_output.parent.mkdir(parents=True, exist_ok=True)
    write_json(report_output, report)
    return report


def fit_release(
    canonical: Path,
    arctic_vectors: Path,
    study_vectors: Path,
    arctic_space_contract: Path,
    study_manifest: Path,
    validation_report: Path,
    source_dims: int,
    target_dims: int,
    transform_output: Path,
    contract_output: Path,
) -> dict[str, object]:
    validation = json.loads(validation_report.read_text(encoding="utf-8"))
    if not validation.get("metrics", {}).get("viable"):
        raise ValueError("held-out-book validation did not pass the pre-registered gate")
    bound_inputs = {
        "pipeline_sha256": sha256_file(Path(__file__)),
        "canonical_sha256": sha256_file(canonical),
        "arctic_vectors_sha256": sha256_file(arctic_vectors),
        "study_vectors_sha256": sha256_file(study_vectors),
        "arctic_space_contract_sha256": sha256_file(arctic_space_contract),
        "study_extract_manifest_sha256": sha256_file(study_manifest),
    }
    mismatches = [
        key for key, digest in bound_inputs.items() if validation.get(key) != digest
    ]
    if mismatches:
        raise ValueError(
            "validation report does not bind the requested release inputs: "
            + ", ".join(mismatches)
        )
    canonical_rows = read_canonical(canonical)
    source = load_matrix(arctic_vectors, len(canonical_rows), source_dims)
    target = load_matrix(study_vectors, len(canonical_rows), target_dims)
    indices = np.arange(len(canonical_rows), dtype=np.int64)
    transform, singular_values = fit_rectangular_procrustes(source, target, indices)
    save_transform(transform_output, transform)
    contract = contract_for_transform(
        transform_output,
        source_dims,
        target_dims,
        arctic_vectors,
        study_vectors,
        canonical,
        arctic_space_contract,
        study_manifest,
        {
            "mode": "all-corpus-release-candidate",
            "rows": len(canonical_rows),
            "validation_report_sha256": sha256_file(validation_report),
            "singular_value_largest": float(singular_values[0]),
            "singular_value_smallest": float(singular_values[-1]),
        },
    )
    write_json(contract_output, contract)
    return contract


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    extract_parser = subparsers.add_parser("extract")
    extract_parser.add_argument("--database", type=Path, required=True)
    extract_parser.add_argument("--canonical", type=Path, required=True)
    extract_parser.add_argument("--output", type=Path, required=True)
    extract_parser.add_argument("--manifest-output", type=Path, required=True)
    extract_parser.add_argument("--dims", type=int, default=1536)
    extract_parser.add_argument("--expected-database-sha256")

    validate_parser = subparsers.add_parser("validate")
    validate_parser.add_argument("--canonical", type=Path, required=True)
    validate_parser.add_argument("--arctic-vectors", type=Path, required=True)
    validate_parser.add_argument("--study-vectors", type=Path, required=True)
    validate_parser.add_argument("--arctic-space-contract", type=Path, required=True)
    validate_parser.add_argument("--study-manifest", type=Path, required=True)
    validate_parser.add_argument("--source-dims", type=int, default=256)
    validate_parser.add_argument("--target-dims", type=int, default=1536)
    validate_parser.add_argument("--fold-count", type=int, default=4)
    validate_parser.add_argument("--holdout-fold", type=int, default=0)
    validate_parser.add_argument("--queries-per-book", type=int, default=32)
    validate_parser.add_argument("--global-candidates", type=int, default=4096)
    validate_parser.add_argument("--report-output", type=Path, required=True)
    validate_parser.add_argument("--transform-output", type=Path)
    validate_parser.add_argument("--contract-output", type=Path)
    validate_parser.add_argument("--save-failed-transform", action="store_true")

    release_parser = subparsers.add_parser("fit-release")
    release_parser.add_argument("--canonical", type=Path, required=True)
    release_parser.add_argument("--arctic-vectors", type=Path, required=True)
    release_parser.add_argument("--study-vectors", type=Path, required=True)
    release_parser.add_argument("--arctic-space-contract", type=Path, required=True)
    release_parser.add_argument("--study-manifest", type=Path, required=True)
    release_parser.add_argument("--validation-report", type=Path, required=True)
    release_parser.add_argument("--source-dims", type=int, default=256)
    release_parser.add_argument("--target-dims", type=int, default=1536)
    release_parser.add_argument("--transform-output", type=Path, required=True)
    release_parser.add_argument("--contract-output", type=Path, required=True)

    args = parser.parse_args()
    if args.command == "extract":
        result = extract(
            args.database,
            args.canonical,
            args.output,
            args.manifest_output,
            args.dims,
            args.expected_database_sha256,
        )
        print(
            f"wrote {result['reference_audit']['matched_canonical_rows']} aligned "
            f"Study vectors ({result['output_bytes']} bytes) -> {args.output}"
        )
    elif args.command == "validate":
        result = validate(
            args.canonical,
            args.arctic_vectors,
            args.study_vectors,
            args.arctic_space_contract,
            args.study_manifest,
            args.source_dims,
            args.target_dims,
            args.fold_count,
            args.holdout_fold,
            args.queries_per_book,
            args.global_candidates,
            args.report_output,
            args.transform_output,
            args.contract_output,
            args.save_failed_transform,
        )
        print(
            f"held-out-book alignment viable={result['metrics']['viable']} -> "
            f"{args.report_output}"
        )
    else:
        result = fit_release(
            args.canonical,
            args.arctic_vectors,
            args.study_vectors,
            args.arctic_space_contract,
            args.study_manifest,
            args.validation_report,
            args.source_dims,
            args.target_dims,
            args.transform_output,
            args.contract_output,
        )
        print(f"wrote release-candidate transform {result['contract_id']}")


if __name__ == "__main__":
    main()
