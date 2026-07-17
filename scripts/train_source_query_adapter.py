#!/usr/bin/env python3
"""Train a compact nonlinear query adapter from source-authored alignments.

The only supervision is the aligned Study Note query embedding and canonical
verse embedding corpus.  A deterministic set of whole Bible books is excluded
from gradient updates and used for epoch/hyperparameter selection.  Exact
duplicate Study Note documents are source-defined multi-positive ranges.

No evaluation prompt, report, gold reference, or candidate score is accepted
as an input.  Paths beneath this repository's ``eval`` directory are rejected
before any input is opened.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import sys
from typing import Iterable

import numpy as np
import torch
from torch import nn

from source_query_adapter import (
    AdapterParameters,
    apply_adapter,
    checked_matmul,
    content_sha256,
    save_content_addressed,
    serialize_adapter,
)


PROJECT_ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_EVAL_ROOT = PROJECT_ROOT / "eval"
DEFAULT_METADATA = Path("/private/tmp/bsb-study-note-dense-view-v1.jsonl")
DEFAULT_SOURCE_VECTORS = Path("/private/tmp/arctic-m256-bsb-study-note-query.f32")
DEFAULT_TARGET_VECTORS = Path("/private/tmp/arctic-m256-bsb-verse.f32")
REFERENCE_RE = re.compile(
    r"^(?P<book>.+?) (?P<chapter>[1-9][0-9]*):(?P<verse>[1-9][0-9]*)$"
)
MANIFEST_SCHEMA = "verse-recall-source-query-adapter-corpus-validation-v1"
SPLIT_POLICY = "sorted-canonical-books-every-nth-held-out-v1"
_TORCH_CONFIGURED = False


@dataclass(frozen=True)
class Candidate:
    name: str
    hidden_dim: int
    learning_rate: float


@dataclass(frozen=True)
class LossConfig:
    temperature: float
    hard_negative_count: int
    hard_negative_margin: float
    hard_negative_weight: float
    identity_weight: float
    weight_decay: float
    gradient_clip: float
    residual_scale: float


@dataclass
class Corpus:
    rows: list[dict]
    source: np.ndarray
    target: np.ndarray
    eligible: np.ndarray
    books: list[str]
    row_books: list[str]
    group_ids: np.ndarray
    group_members: dict[int, np.ndarray]
    group_text_digests: dict[int, str]


@dataclass
class CandidateResult:
    candidate: Candidate
    selected_epoch: int
    selected_metrics: dict
    parameters: AdapterParameters
    history: list[dict]


def _absolute_without_symlinks(path: Path) -> Path:
    return Path(os.path.abspath(os.fspath(path)))


def _is_within(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def reject_evaluation_path(path: Path, *, must_exist: bool) -> Path:
    """Reject lexical and symlink-resolved paths beneath ``eval/``.

    The lexical check happens first, so a forbidden path is rejected without
    probing whether the named file exists.  Existing inputs are then resolved
    strictly to prevent a symlink from bypassing the boundary.
    """

    root = _absolute_without_symlinks(FORBIDDEN_EVAL_ROOT)
    lexical = _absolute_without_symlinks(path)
    if _is_within(lexical, root):
        raise ValueError(f"evaluation paths are forbidden: {path}")
    if must_exist:
        resolved = path.resolve(strict=True)
        if _is_within(resolved, root):
            raise ValueError(f"evaluation paths are forbidden: {path}")
        if not resolved.is_file():
            raise ValueError(f"training input is not a regular file: {path}")
        return resolved
    # ``strict=False`` resolves every existing symlink prefix, including an
    # output directory alias, without requiring the destination file to exist.
    resolved = path.resolve(strict=False)
    if _is_within(resolved, root):
        raise ValueError(f"evaluation paths are forbidden: {path}")
    return resolved


def validate_source_paths(
    metadata: Path,
    source_vectors: Path,
    target_vectors: Path,
    *,
    allow_custom_source_inputs: bool,
) -> tuple[Path, Path, Path]:
    # Validate every lexical path before resolving or opening any one input.
    for path in (metadata, source_vectors, target_vectors):
        reject_evaluation_path(path, must_exist=False)
    resolved = tuple(
        reject_evaluation_path(path, must_exist=True)
        for path in (metadata, source_vectors, target_vectors)
    )
    if not allow_custom_source_inputs:
        expected = tuple(
            path.resolve(strict=True)
            for path in (
                DEFAULT_METADATA,
                DEFAULT_SOURCE_VECTORS,
                DEFAULT_TARGET_VECTORS,
            )
        )
        if resolved != expected:
            raise ValueError(
                "this build is locked to the three declared Study Note/canonical "
                "source files; use --allow-custom-source-inputs only for isolated "
                "toy or future source-corpus work"
            )
    return resolved  # type: ignore[return-value]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def reference_book(reference: str) -> str:
    match = REFERENCE_RE.fullmatch(reference.strip())
    if match is None:
        raise ValueError(f"invalid canonical reference {reference!r}")
    return match.group("book")


def read_metadata(path: Path) -> list[dict]:
    rows = []
    with path.open("r", encoding="utf-8") as source:
        for line_number, line in enumerate(source, 1):
            if not line.strip():
                raise ValueError(f"metadata row {line_number} is empty")
            value = json.loads(line)
            if not isinstance(value, dict):
                raise ValueError(f"metadata row {line_number} is not an object")
            if set(value) != {"ref", "text", "view"}:
                raise ValueError(
                    f"metadata row {line_number} does not match the dense-view schema"
                )
            if not isinstance(value["ref"], str):
                raise ValueError(f"metadata row {line_number} has no reference")
            reference_book(value["ref"])
            if not isinstance(value["text"], str) or not value["text"].strip():
                raise ValueError(f"metadata row {line_number} has no source text")
            if value["view"] not in {"study-note", "canonical-fallback"}:
                raise ValueError(f"metadata row {line_number} has an invalid view")
            rows.append(value)
    if not rows:
        raise ValueError("metadata is empty")
    return rows


def read_f32_matrix(path: Path, dimensions: int) -> np.ndarray:
    if dimensions <= 0:
        raise ValueError("dimensions must be positive")
    values = np.fromfile(path, dtype="<f4")
    if values.size == 0 or values.size % dimensions:
        raise ValueError(
            f"{path} is not a non-empty row-major {dimensions}D float32 matrix"
        )
    matrix = np.asarray(values.reshape(-1, dimensions), dtype=np.float32, order="C")
    if not np.all(np.isfinite(matrix)):
        raise ValueError(f"{path} contains nonfinite values")
    return matrix


def normalize_rows(vectors: np.ndarray) -> np.ndarray:
    vectors = np.asarray(vectors, dtype=np.float32, order="C")
    norms = np.linalg.norm(vectors, axis=1, keepdims=True)
    if np.any(~np.isfinite(vectors)) or np.any(norms <= np.float32(1e-12)):
        raise ValueError("embedding matrix contains a nonfinite or zero-norm row")
    return np.asarray(vectors / norms, dtype=np.float32, order="C")


def source_groups(
    rows: list[dict], eligible: np.ndarray
) -> tuple[np.ndarray, dict[int, np.ndarray], dict[int, str]]:
    """Assign exact source-note documents to deterministic equivalence groups."""

    texts = {rows[int(index)]["text"] for index in np.flatnonzero(eligible)}
    ordered_texts = sorted(
        texts,
        key=lambda text: (hashlib.sha256(text.encode("utf-8")).digest(), text),
    )
    text_to_group = {text: group for group, text in enumerate(ordered_texts)}
    group_ids = np.full(len(rows), -1, dtype=np.int64)
    members: dict[int, list[int]] = defaultdict(list)
    digests: dict[int, str] = {}
    for text, group in text_to_group.items():
        digests[group] = hashlib.sha256(text.encode("utf-8")).hexdigest()
    for index in np.flatnonzero(eligible):
        group = text_to_group[rows[int(index)]["text"]]
        group_ids[int(index)] = group
        members[group].append(int(index))
    return (
        group_ids,
        {
            group: np.asarray(indices, dtype=np.int64)
            for group, indices in members.items()
        },
        digests,
    )


def load_corpus(
    metadata: Path, source_vectors: Path, target_vectors: Path, dimensions: int
) -> Corpus:
    rows = read_metadata(metadata)
    source = normalize_rows(read_f32_matrix(source_vectors, dimensions))
    target = normalize_rows(read_f32_matrix(target_vectors, dimensions))
    if len(rows) != len(source) or len(rows) != len(target):
        raise ValueError(
            "metadata, Study Note query vectors, and canonical verse vectors "
            "must have identical aligned row counts"
        )
    eligible = np.asarray(
        [row["view"] == "study-note" for row in rows], dtype=bool
    )
    if not eligible.any() or eligible.all():
        raise ValueError("corpus must contain Study Note and canonical fallback rows")
    row_books = [reference_book(row["ref"]) for row in rows]
    books = sorted(set(row_books))
    if len(books) < 10:
        raise ValueError("too few books for a whole-book-held-out split")
    group_ids, group_members, group_text_digests = source_groups(rows, eligible)
    return Corpus(
        rows=rows,
        source=source,
        target=target,
        eligible=eligible,
        books=books,
        row_books=row_books,
        group_ids=group_ids,
        group_members=group_members,
        group_text_digests=group_text_digests,
    )


def whole_book_split(
    corpus: Corpus, stride: int, offset: int
) -> tuple[np.ndarray, np.ndarray, list[str]]:
    if stride < 3:
        raise ValueError("holdout stride must be at least 3")
    if offset < 0 or offset >= stride:
        raise ValueError("holdout offset must be in [0, stride)")
    heldout_books = corpus.books[offset::stride]
    if not heldout_books:
        raise ValueError("whole-book holdout selected no books")
    heldout = set(heldout_books)
    train_indices = np.asarray(
        [
            int(index)
            for index in np.flatnonzero(corpus.eligible)
            if corpus.row_books[int(index)] not in heldout
        ],
        dtype=np.int64,
    )
    validation_indices = np.asarray(
        [
            int(index)
            for index in np.flatnonzero(corpus.eligible)
            if corpus.row_books[int(index)] in heldout
        ],
        dtype=np.int64,
    )
    if not len(train_indices) or not len(validation_indices):
        raise ValueError("whole-book split produced an empty partition")

    train_groups = set(corpus.group_ids[train_indices].tolist())
    validation_groups = set(corpus.group_ids[validation_indices].tolist())
    overlap = train_groups & validation_groups
    if overlap:
        examples = sorted(corpus.group_text_digests[group] for group in overlap)[:3]
        raise ValueError(
            "exact source notes cross the whole-book boundary; refuse leakage "
            f"({len(overlap)} groups, examples={examples})"
        )
    return train_indices, validation_indices, heldout_books


def representative_queries(
    indices: np.ndarray, group_ids: np.ndarray, limit: int | None
) -> np.ndarray:
    by_group: dict[int, int] = {}
    for index in indices:
        group = int(group_ids[int(index)])
        by_group[group] = min(int(index), by_group.get(group, int(index)))
    representatives = sorted(
        by_group.values(),
        key=lambda index: (
            hashlib.sha256(str(index).encode("ascii")).digest(),
            index,
        ),
    )
    if limit is not None:
        if limit <= 0:
            raise ValueError("selection validation limit must be positive")
        representatives = representatives[:limit]
    return np.asarray(representatives, dtype=np.int64)


def training_group_batches(
    train_indices: np.ndarray,
    group_ids: np.ndarray,
    batch_size: int,
    seed: int,
    epoch: int,
) -> Iterable[np.ndarray]:
    """Yield group-complete batches so an exact range is never a negative."""

    if batch_size <= 1:
        raise ValueError("training batch size must be greater than one")
    members: dict[int, list[int]] = defaultdict(list)
    for index in train_indices:
        members[int(group_ids[int(index)])].append(int(index))
    groups = np.asarray(sorted(members), dtype=np.int64)
    permutation = np.random.Generator(
        np.random.PCG64(seed + epoch * 1_000_003)
    ).permutation(len(groups))
    current: list[int] = []
    for position in permutation:
        group_members = members[int(groups[int(position)])]
        if current and len(current) + len(group_members) > batch_size:
            yield np.asarray(current, dtype=np.int64)
            current = []
        current.extend(group_members)
    if current:
        yield np.asarray(current, dtype=np.int64)


class ResidualQueryAdapter(nn.Module):
    def __init__(
        self, dimensions: int, hidden_dim: int, residual_scale: float, seed: int
    ) -> None:
        super().__init__()
        if dimensions <= 0 or hidden_dim <= 0:
            raise ValueError("adapter dimensions must be positive")
        self.down = nn.Linear(dimensions, hidden_dim, bias=True)
        self.up = nn.Linear(hidden_dim, dimensions, bias=True)
        self.residual_scale = float(residual_scale)
        generator = torch.Generator(device="cpu")
        generator.manual_seed(seed)
        with torch.no_grad():
            self.down.weight.copy_(
                torch.randn(
                    self.down.weight.shape,
                    generator=generator,
                    dtype=torch.float32,
                )
                * 0.02
            )
            self.down.bias.zero_()
            self.up.weight.zero_()
            self.up.bias.zero_()

    @staticmethod
    def tanh_gelu(value: torch.Tensor) -> torch.Tensor:
        return 0.5 * value * (
            1.0
            + torch.tanh(
                0.7978845608028654 * (value + 0.044715 * value * value * value)
            )
        )

    def forward(self, vectors: torch.Tensor) -> torch.Tensor:
        vectors = torch.nn.functional.normalize(vectors, p=2, dim=1)
        hidden = self.tanh_gelu(self.down(vectors))
        adapted = vectors + self.residual_scale * self.up(hidden)
        return torch.nn.functional.normalize(adapted, p=2, dim=1)

    def numpy_parameters(self) -> AdapterParameters:
        return AdapterParameters(
            down_weight=np.asarray(
                self.down.weight.detach().cpu().numpy().T,
                dtype=np.float32,
                order="C",
            ).copy(),
            down_bias=np.asarray(
                self.down.bias.detach().cpu().numpy(), dtype=np.float32
            ).copy(),
            up_weight=np.asarray(
                self.up.weight.detach().cpu().numpy().T,
                dtype=np.float32,
                order="C",
            ).copy(),
            up_bias=np.asarray(
                self.up.bias.detach().cpu().numpy(), dtype=np.float32
            ).copy(),
            residual_scale=self.residual_scale,
        )


def configure_deterministic_torch() -> None:
    global _TORCH_CONFIGURED
    if _TORCH_CONFIGURED:
        return
    torch.set_num_threads(1)
    try:
        torch.set_num_interop_threads(1)
    except RuntimeError:
        # Another library may already have initialized the interop pool.  The
        # trainer never uses it, and intra-op execution remains single-threaded.
        pass
    torch.use_deterministic_algorithms(True)
    _TORCH_CONFIGURED = True


def multi_positive_loss(
    adapted: torch.Tensor,
    source: torch.Tensor,
    targets: torch.Tensor,
    groups: torch.Tensor,
    global_group_sizes: torch.Tensor,
    config: LossConfig,
) -> tuple[torch.Tensor, dict[str, float]]:
    similarities = adapted @ targets.T
    positive_mask = groups[:, None].eq(groups[None, :])
    if not torch.all(positive_mask.any(dim=1)):
        raise RuntimeError("training batch contains a query without a positive")
    logits = similarities / config.temperature
    negative_infinity = torch.tensor(
        float("-inf"), dtype=logits.dtype, device=logits.device
    )
    numerator = torch.logsumexp(
        logits.masked_fill(~positive_mask, negative_infinity), dim=1
    )
    denominator = torch.logsumexp(logits, dim=1)
    weights = 1.0 / global_group_sizes.to(dtype=logits.dtype)
    weight_sum = weights.sum()
    contrastive_per_query = denominator - numerator
    contrastive = (contrastive_per_query * weights).sum() / weight_sum

    negative_counts = (~positive_mask).sum(dim=1)
    available_negatives = int(negative_counts.min().item())
    if available_negatives:
        count = min(config.hard_negative_count, available_negatives)
        hard_negatives = torch.topk(
            similarities.masked_fill(positive_mask, negative_infinity),
            k=count,
            dim=1,
        ).values
        best_positive = similarities.masked_fill(
            ~positive_mask, negative_infinity
        ).max(dim=1).values
        hard_per_query = torch.relu(
            config.hard_negative_margin
            + hard_negatives
            - best_positive[:, None]
        ).mean(dim=1)
        hard_negative = (hard_per_query * weights).sum() / weight_sum
    else:
        hard_negative = similarities.sum() * 0.0

    identity_per_query = 1.0 - (adapted * source).sum(dim=1)
    identity = (identity_per_query * weights).sum() / weight_sum
    total = (
        contrastive
        + config.hard_negative_weight * hard_negative
        + config.identity_weight * identity
    )
    return total, {
        "contrastive": float(contrastive.detach().cpu()),
        "hard_negative": float(hard_negative.detach().cpu()),
        "identity": float(identity.detach().cpu()),
        "total": float(total.detach().cpu()),
    }


def _model_seed(base_seed: int, hidden_dim: int) -> int:
    return base_seed + hidden_dim * 1009


def initialize_model(
    dimensions: int, candidate: Candidate, config: LossConfig, seed: int
) -> ResidualQueryAdapter:
    configure_deterministic_torch()
    return ResidualQueryAdapter(
        dimensions,
        candidate.hidden_dim,
        config.residual_scale,
        _model_seed(seed, candidate.hidden_dim),
    )


def train_one_epoch(
    model: ResidualQueryAdapter,
    optimizer: torch.optim.Optimizer,
    source_tensor: torch.Tensor,
    target_tensor: torch.Tensor,
    group_tensor: torch.Tensor,
    group_size_tensor: torch.Tensor,
    train_indices: np.ndarray,
    batch_size: int,
    seed: int,
    epoch: int,
    config: LossConfig,
) -> dict[str, float]:
    model.train()
    totals: dict[str, float] = defaultdict(float)
    group_count = 0
    batch_count = 0
    for indices in training_group_batches(
        train_indices, group_tensor.numpy(), batch_size, seed, epoch
    ):
        batch_index = torch.from_numpy(indices)
        source = source_tensor.index_select(0, batch_index)
        targets = target_tensor.index_select(0, batch_index)
        groups = group_tensor.index_select(0, batch_index)
        group_sizes = group_size_tensor.index_select(0, batch_index)
        optimizer.zero_grad(set_to_none=True)
        adapted = model(source)
        loss, parts = multi_positive_loss(
            adapted, source, targets, groups, group_sizes, config
        )
        if not bool(torch.isfinite(loss)):
            raise RuntimeError("adapter training produced a nonfinite loss")
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), config.gradient_clip)
        optimizer.step()
        groups_in_batch = int(torch.unique(groups).numel())
        for name, value in parts.items():
            totals[name] += value * groups_in_batch
        group_count += groups_in_batch
        batch_count += 1
    if not batch_count or not group_count:
        raise RuntimeError("adapter training produced no batches")
    return {
        **{name: value / group_count for name, value in totals.items()},
        "batches": batch_count,
        "unique_note_groups": group_count,
    }


def exact_retrieval_metrics(
    source: np.ndarray,
    target_corpus: np.ndarray,
    query_indices: np.ndarray,
    group_ids: np.ndarray,
    group_members: dict[int, np.ndarray],
    parameters: AdapterParameters | None,
    batch_size: int,
) -> dict:
    if batch_size <= 0:
        raise ValueError("retrieval batch size must be positive")
    queries = source[query_indices]
    adapted = queries if parameters is None else apply_adapter(queries, parameters)
    all_indices = np.arange(len(target_corpus), dtype=np.int64)
    ranks: list[int] = []
    positive_scores: list[float] = []
    negative_gaps: list[float] = []
    for start in range(0, len(adapted), batch_size):
        query = adapted[start : start + batch_size]
        indices = query_indices[start : start + batch_size]
        scores = checked_matmul(
            query, target_corpus.T, "source-held-out retrieval scoring"
        )
        for offset, index in enumerate(indices):
            positives = group_members[int(group_ids[int(index)])]
            positive_values = scores[offset, positives]
            best_score = float(positive_values.max())
            best_positive_indices = positives[positive_values == best_score]
            best_positive_index = int(best_positive_indices.min())
            greater = int(np.count_nonzero(scores[offset] > best_score))
            tied_before = int(
                np.count_nonzero(
                    (scores[offset] == best_score)
                    & (all_indices < best_positive_index)
                )
            )
            ranks.append(1 + greater + tied_before)
            positive_scores.append(best_score)
            negative_scores = scores[offset].copy()
            negative_scores[positives] = -np.inf
            negative_gaps.append(best_score - float(negative_scores.max()))
    rank_array = np.asarray(ranks, dtype=np.int64)
    identity_cosines = np.sum(adapted * queries, axis=1)
    return {
        "queries": int(len(rank_array)),
        "recall_at_1": float(np.mean(rank_array <= 1)),
        "recall_at_3": float(np.mean(rank_array <= 3)),
        "recall_at_10": float(np.mean(rank_array <= 10)),
        "recall_at_50": float(np.mean(rank_array <= 50)),
        "recall_at_100": float(np.mean(rank_array <= 100)),
        "mrr": float(np.mean(1.0 / rank_array)),
        "median_rank": float(np.median(rank_array)),
        "mean_rank": float(np.mean(rank_array)),
        "mean_positive_cosine": float(np.mean(positive_scores)),
        "positive_over_best_negative": float(
            np.mean(np.asarray(negative_gaps) > 0.0)
        ),
        "mean_best_negative_gap": float(np.mean(negative_gaps)),
        "mean_identity_cosine": float(np.mean(identity_cosines)),
    }


def selection_key(metrics: dict, hidden_dim: int, epoch: int, name: str) -> tuple:
    # The ordering is fixed before fitting.  Retrieval breadth is primary;
    # identity preservation and compactness break otherwise equal outcomes.
    return (
        metrics["recall_at_3"],
        metrics["recall_at_10"],
        metrics["mrr"],
        metrics["recall_at_1"],
        metrics["mean_identity_cosine"],
        -hidden_dim,
        -epoch,
        name,
    )


def fit_candidate(
    corpus: Corpus,
    train_indices: np.ndarray,
    selection_indices: np.ndarray,
    candidate: Candidate,
    loss_config: LossConfig,
    epochs: int,
    train_batch_size: int,
    validation_batch_size: int,
    seed: int,
) -> CandidateResult:
    model = initialize_model(corpus.source.shape[1], candidate, loss_config, seed)
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=candidate.learning_rate,
        weight_decay=loss_config.weight_decay,
        foreach=False,
        fused=False,
    )
    source_tensor = torch.from_numpy(corpus.source)
    target_tensor = torch.from_numpy(corpus.target)
    group_tensor = torch.from_numpy(corpus.group_ids)
    group_sizes = np.ones(len(corpus.rows), dtype=np.int64)
    for members in corpus.group_members.values():
        group_sizes[members] = len(members)
    group_size_tensor = torch.from_numpy(group_sizes)

    best_epoch = -1
    best_metrics: dict | None = None
    best_parameters: AdapterParameters | None = None
    history = []
    for epoch in range(1, epochs + 1):
        training = train_one_epoch(
            model,
            optimizer,
            source_tensor,
            target_tensor,
            group_tensor,
            group_size_tensor,
            train_indices,
            train_batch_size,
            seed,
            epoch,
            loss_config,
        )
        parameters = model.numpy_parameters()
        metrics = exact_retrieval_metrics(
            corpus.source,
            corpus.target,
            selection_indices,
            corpus.group_ids,
            corpus.group_members,
            parameters,
            validation_batch_size,
        )
        history.append({"epoch": epoch, "training": training, "retrieval": metrics})
        if best_metrics is None or selection_key(
            metrics, candidate.hidden_dim, epoch, candidate.name
        ) > selection_key(
            best_metrics, candidate.hidden_dim, best_epoch, candidate.name
        ):
            best_epoch = epoch
            best_metrics = metrics
            best_parameters = parameters
    if best_metrics is None or best_parameters is None:
        raise RuntimeError("candidate fitting selected no epoch")
    return CandidateResult(
        candidate=candidate,
        selected_epoch=best_epoch,
        selected_metrics=best_metrics,
        parameters=best_parameters,
        history=history,
    )


def reproduce_parameters(
    corpus: Corpus,
    train_indices: np.ndarray,
    candidate: Candidate,
    loss_config: LossConfig,
    epoch_count: int,
    train_batch_size: int,
    seed: int,
) -> AdapterParameters:
    model = initialize_model(corpus.source.shape[1], candidate, loss_config, seed)
    if epoch_count == 0:
        return model.numpy_parameters()
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=candidate.learning_rate,
        weight_decay=loss_config.weight_decay,
        foreach=False,
        fused=False,
    )
    source_tensor = torch.from_numpy(corpus.source)
    target_tensor = torch.from_numpy(corpus.target)
    group_tensor = torch.from_numpy(corpus.group_ids)
    group_sizes = np.ones(len(corpus.rows), dtype=np.int64)
    for members in corpus.group_members.values():
        group_sizes[members] = len(members)
    group_size_tensor = torch.from_numpy(group_sizes)
    for epoch in range(1, epoch_count + 1):
        train_one_epoch(
            model,
            optimizer,
            source_tensor,
            target_tensor,
            group_tensor,
            group_size_tensor,
            train_indices,
            train_batch_size,
            seed,
            epoch,
            loss_config,
        )
    return model.numpy_parameters()


def parse_candidate(value: str) -> Candidate:
    try:
        name, hidden, learning_rate = value.split(":")
        candidate = Candidate(name, int(hidden), float(learning_rate))
    except (TypeError, ValueError) as error:
        raise argparse.ArgumentTypeError(
            "candidate must be NAME:HIDDEN_DIM:LEARNING_RATE"
        ) from error
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", candidate.name):
        raise argparse.ArgumentTypeError("candidate name must be lowercase kebab-case")
    if candidate.hidden_dim <= 0 or candidate.learning_rate <= 0:
        raise argparse.ArgumentTypeError("candidate dimensions/rate must be positive")
    return candidate


def _group_counts(corpus: Corpus, indices: np.ndarray) -> int:
    return len(set(corpus.group_ids[indices].tolist()))


def macro_average_book_metrics(per_book_metrics: dict[str, dict]) -> dict:
    if not per_book_metrics:
        raise ValueError("cannot average an empty heldout-book metric set")
    result = {}
    for lane in ("identity_baseline", "selected_adapter"):
        first = next(iter(per_book_metrics.values()))[lane]
        result[lane] = {
            name: float(
                np.mean(
                    [metrics[lane][name] for metrics in per_book_metrics.values()]
                )
            )
            for name, value in first.items()
            if name != "queries" and isinstance(value, (int, float))
        }
    return result


def run(args: argparse.Namespace) -> dict:
    metadata, source_vectors, target_vectors = validate_source_paths(
        args.metadata,
        args.source_vectors,
        args.target_vectors,
        allow_custom_source_inputs=args.allow_custom_source_inputs,
    )
    output_dir = reject_evaluation_path(args.output_dir, must_exist=False)
    if args.manifest is not None:
        reject_evaluation_path(args.manifest, must_exist=False)
    corpus = load_corpus(metadata, source_vectors, target_vectors, args.dimensions)
    train_indices, heldout_indices, heldout_books = whole_book_split(
        corpus, args.holdout_stride, args.holdout_offset
    )
    selection_indices = representative_queries(
        heldout_indices, corpus.group_ids, args.selection_limit
    )
    full_heldout_queries = representative_queries(
        heldout_indices, corpus.group_ids, None
    )
    candidates = args.candidate or [
        Candidate("lowrank32-lr1", 32, 0.001),
        Candidate("lowrank32-lr3", 32, 0.003),
    ]
    if len({candidate.name for candidate in candidates}) != len(candidates):
        raise ValueError("candidate names must be unique")
    if any(candidate.hidden_dim > args.dimensions for candidate in candidates):
        raise ValueError("candidate hidden dimension cannot exceed input dimensions")
    loss_config = LossConfig(
        temperature=args.temperature,
        hard_negative_count=args.hard_negative_count,
        hard_negative_margin=args.hard_negative_margin,
        hard_negative_weight=args.hard_negative_weight,
        identity_weight=args.identity_weight,
        weight_decay=args.weight_decay,
        gradient_clip=args.gradient_clip,
        residual_scale=args.residual_scale,
    )
    if args.epochs <= 0:
        raise ValueError("epochs must be positive")

    baseline_metrics = exact_retrieval_metrics(
        corpus.source,
        corpus.target,
        selection_indices,
        corpus.group_ids,
        corpus.group_members,
        None,
        args.validation_batch_size,
    )
    results = [
        fit_candidate(
            corpus,
            train_indices,
            selection_indices,
            candidate,
            loss_config,
            args.epochs,
            args.train_batch_size,
            args.validation_batch_size,
            args.seed,
        )
        for candidate in candidates
    ]
    selected_result = max(
        results,
        key=lambda result: selection_key(
            result.selected_metrics,
            result.candidate.hidden_dim,
            result.selected_epoch,
            result.candidate.name,
        ),
    )

    # Never ship a source-held-out regression merely to force a learned model.
    # Epoch zero is represented by the smallest candidate's exact identity
    # initialization (zero up-projection).
    identity_candidate = min(
        candidates, key=lambda candidate: (candidate.hidden_dim, candidate.name)
    )
    identity_parameters = initialize_model(
        args.dimensions, identity_candidate, loss_config, args.seed
    ).numpy_parameters()
    selected_is_identity = selection_key(
        baseline_metrics, identity_candidate.hidden_dim, 0, "identity"
    ) >= selection_key(
        selected_result.selected_metrics,
        selected_result.candidate.hidden_dim,
        selected_result.selected_epoch,
        selected_result.candidate.name,
    )
    if selected_is_identity:
        selected_name = "identity"
        selected_candidate = identity_candidate
        selected_epoch = 0
        selected_metrics = baseline_metrics
        selected_parameters = identity_parameters
    else:
        selected_name = selected_result.candidate.name
        selected_candidate = selected_result.candidate
        selected_epoch = selected_result.selected_epoch
        selected_metrics = selected_result.selected_metrics
        selected_parameters = selected_result.parameters

    artifact = save_content_addressed(output_dir, selected_parameters)
    artifact_bytes = artifact.read_bytes()
    artifact_digest = content_sha256(artifact_bytes)
    reproducible = None
    if args.reproducibility_check:
        reproduced = reproduce_parameters(
            corpus,
            train_indices,
            selected_candidate,
            loss_config,
            selected_epoch,
            args.train_batch_size,
            args.seed,
        )
        reproduced_bytes = serialize_adapter(reproduced)
        reproducible = reproduced_bytes == artifact_bytes
        if not reproducible:
            raise RuntimeError("deterministic full-corpus reproducibility check failed")

    full_baseline_metrics = exact_retrieval_metrics(
        corpus.source,
        corpus.target,
        full_heldout_queries,
        corpus.group_ids,
        corpus.group_members,
        None,
        args.validation_batch_size,
    )
    full_selected_metrics = exact_retrieval_metrics(
        corpus.source,
        corpus.target,
        full_heldout_queries,
        corpus.group_ids,
        corpus.group_members,
        selected_parameters,
        args.validation_batch_size,
    )
    per_book_metrics = {}
    for book in heldout_books:
        book_rows = np.asarray(
            [
                int(index)
                for index in heldout_indices
                if corpus.row_books[int(index)] == book
            ],
            dtype=np.int64,
        )
        book_queries = representative_queries(book_rows, corpus.group_ids, None)
        per_book_metrics[book] = {
            "identity_baseline": exact_retrieval_metrics(
                corpus.source,
                corpus.target,
                book_queries,
                corpus.group_ids,
                corpus.group_members,
                None,
                args.validation_batch_size,
            ),
            "selected_adapter": exact_retrieval_metrics(
                corpus.source,
                corpus.target,
                book_queries,
                corpus.group_ids,
                corpus.group_members,
                selected_parameters,
                args.validation_batch_size,
            ),
        }
    duplicate_groups = [
        members for members in corpus.group_members.values() if len(members) > 1
    ]
    manifest = {
        "schema": MANIFEST_SCHEMA,
        "supervision": {
            "policy": "source-authored-study-note-to-canonical-verse-only",
            "evaluation_inputs_rejected": True,
            "custom_source_inputs_enabled": bool(args.allow_custom_source_inputs),
            "prompt_or_query_evaluation_metrics_accessed": False,
            "multi_positive_policy": "exact-identical-source-note-document-v1",
        },
        "inputs": {
            "metadata": {
                "path": str(metadata),
                "sha256": sha256_file(metadata),
                "bytes": metadata.stat().st_size,
                "rows": len(corpus.rows),
                "study_note_rows": int(corpus.eligible.sum()),
                "canonical_fallback_rows": int((~corpus.eligible).sum()),
            },
            "study_note_query_vectors": {
                "path": str(source_vectors),
                "sha256": sha256_file(source_vectors),
                "bytes": source_vectors.stat().st_size,
                "dimensions": args.dimensions,
                "rows": len(corpus.source),
                "encoder_input_contract": "query-prefixed Study Note dense documents",
            },
            "canonical_verse_vectors": {
                "path": str(target_vectors),
                "sha256": sha256_file(target_vectors),
                "bytes": target_vectors.stat().st_size,
                "dimensions": args.dimensions,
                "rows": len(corpus.target),
                "encoder_input_contract": "canonical BSB verse dense documents",
            },
        },
        "source_note_groups": {
            "unique": len(corpus.group_members),
            "duplicate_groups": len(duplicate_groups),
            "rows_in_duplicate_groups": int(
                sum(len(members) for members in duplicate_groups)
            ),
            "maximum_positive_targets": max(
                len(members) for members in corpus.group_members.values()
            ),
            "groups_crossing_split": 0,
        },
        "split": {
            "policy": SPLIT_POLICY,
            "holdout_stride": args.holdout_stride,
            "holdout_offset": args.holdout_offset,
            "all_books": corpus.books,
            "training_books": [
                book for book in corpus.books if book not in set(heldout_books)
            ],
            "heldout_books": heldout_books,
            "training_rows": len(train_indices),
            "training_unique_note_documents": _group_counts(corpus, train_indices),
            "heldout_rows": len(heldout_indices),
            "heldout_unique_note_documents": len(full_heldout_queries),
            "selection_unique_note_documents": len(selection_indices),
        },
        "training": {
            "architecture": "normalized residual low-rank tanh-GELU MLP",
            "loss": {
                "contrastive": "multi-positive in-batch InfoNCE",
                "hard_negatives": "top-k non-positive in-batch cosine margin",
                "identity_regularization": "cosine drift from unadapted query",
                **asdict(loss_config),
            },
            "epochs_available": args.epochs,
            "train_batch_target": args.train_batch_size,
            "seed": args.seed,
            "candidate_histories": [
                {
                    "candidate": asdict(result.candidate),
                    "selected_epoch": result.selected_epoch,
                    "selected_retrieval": result.selected_metrics,
                    "history": result.history,
                }
                for result in results
            ],
            "selection_baseline": baseline_metrics,
            "selected": {
                "name": selected_name,
                "candidate": asdict(selected_candidate),
                "epoch": selected_epoch,
                "selection_retrieval": selected_metrics,
            },
        },
        "source_validation": {
            "scope": "all unique source-note documents in whole-book holdout",
            "selection_warning": (
                "the same heldout book partition selected the candidate/epoch; "
                "these are source-validation metrics, not an untouched prompt holdout"
            ),
            "identity_baseline": full_baseline_metrics,
            "selected_adapter": full_selected_metrics,
            "macro_average_across_heldout_books": macro_average_book_metrics(
                per_book_metrics
            ),
            "by_heldout_book": per_book_metrics,
        },
        "artifact": {
            "filename": artifact.name,
            "sha256": artifact_digest,
            "bytes": len(artifact_bytes),
            "format": "VRNQAD01 little-endian float32",
            "application": "post-encoder query-only; normalize-residual-normalize",
            "numpy_runtime": "scripts/source_query_adapter.py",
        },
        "reproducibility": {
            "full_training_rerun_byte_identical": reproducible,
            "torch_deterministic_algorithms": True,
            "torch_threads": 1,
            "python": platform.python_version(),
            "numpy": np.__version__,
            "torch": torch.__version__,
            "trainer_sha256": sha256_file(Path(__file__)),
            "numpy_runtime_sha256": sha256_file(
                Path(__file__).with_name("source_query_adapter.py")
            ),
        },
        "limitations": [
            "supervision is explanatory Study Note prose, not arbitrary user queries",
            "candidate and epoch selection reuse the source-heldout partition",
            "no prompt benchmark, safety suite, latency suite, or release threshold was scored",
            "artifact is not wired into Rust, FFI, WASM, or search bindings",
            "source licensing and redistribution policy remain external review concerns",
        ],
    }
    manifest_path = (
        args.manifest
        if args.manifest is not None
        else output_dir / f"{artifact.stem}.manifest.json"
    )
    manifest_path = reject_evaluation_path(manifest_path, must_exist=False)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return {**manifest, "manifest_path": str(manifest_path)}


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata", type=Path, default=DEFAULT_METADATA)
    parser.add_argument(
        "--source-vectors", type=Path, default=DEFAULT_SOURCE_VECTORS
    )
    parser.add_argument(
        "--target-vectors", type=Path, default=DEFAULT_TARGET_VECTORS
    )
    parser.add_argument("--dimensions", type=int, default=256)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("/private/tmp/verse-recall-source-query-adapter"),
    )
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--allow-custom-source-inputs", action="store_true")
    parser.add_argument("--holdout-stride", type=int, default=5)
    parser.add_argument("--holdout-offset", type=int, default=0)
    parser.add_argument("--selection-limit", type=int, default=1024)
    parser.add_argument("--epochs", type=int, default=5)
    parser.add_argument("--train-batch-size", type=int, default=256)
    parser.add_argument("--validation-batch-size", type=int, default=128)
    parser.add_argument("--seed", type=int, default=1729)
    parser.add_argument(
        "--candidate",
        type=parse_candidate,
        action="append",
        help="repeatable NAME:HIDDEN_DIM:LEARNING_RATE",
    )
    parser.add_argument("--temperature", type=float, default=0.07)
    parser.add_argument("--hard-negative-count", type=int, default=8)
    parser.add_argument("--hard-negative-margin", type=float, default=0.04)
    parser.add_argument("--hard-negative-weight", type=float, default=0.25)
    parser.add_argument("--identity-weight", type=float, default=0.20)
    parser.add_argument("--weight-decay", type=float, default=1e-4)
    parser.add_argument("--gradient-clip", type=float, default=1.0)
    parser.add_argument("--residual-scale", type=float, default=1.0)
    parser.add_argument(
        "--reproducibility-check",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    args = parser.parse_args(argv)
    positive_values = (
        args.dimensions,
        args.selection_limit,
        args.epochs,
        args.train_batch_size,
        args.validation_batch_size,
        args.temperature,
        args.hard_negative_count,
        args.hard_negative_margin,
        args.hard_negative_weight,
        args.identity_weight,
        args.gradient_clip,
        args.residual_scale,
    )
    if any(value <= 0 for value in positive_values):
        parser.error("dimensions, limits, epochs, and loss settings must be positive")
    if args.weight_decay < 0:
        parser.error("weight decay cannot be negative")
    return args


def main() -> None:
    report = run(parse_args())
    selected = report["training"]["selected"]
    validation = report["source_validation"]["selected_adapter"]
    print(
        f"selected {selected['name']} epoch {selected['epoch']}; "
        f"whole-book source R@3={validation['recall_at_3']:.4f} "
        f"R@10={validation['recall_at_10']:.4f}; "
        f"artifact={report['artifact']['filename']}; "
        f"manifest={report['manifest_path']}"
    )


if __name__ == "__main__":
    main()
