#!/usr/bin/env python3
"""Build a fixed weighted concatenation of independent embedding spaces.

For normalized component vectors ``q_i`` and ``d_i`` with weights ``w_i``
that sum to one, the emitted vectors are ``concat(sqrt(w_i) * q_i)`` and
``concat(sqrt(w_i) * d_i)``. Their dot product is therefore exactly the
weighted mean of component cosine similarities (apart from Float32 rounding).

The mechanism is source-only: it consumes vectors and prompt-bound query
records, never prompt gold references or retrieval results.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

import numpy as np


CONTRACT_SCHEMA = "verse-recall-concatenated-embedding-space-v1"
TRANSFORM = "component-l2-sqrt-weight-concat-l2-f32-v1"
PROMPT_DIGEST = "prompt_utf8_sha256"
QUERY_SEGMENT_PLANNER = "verse-recall-clause-v2"


def is_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
        and any(character != "0" for character in value)
    )


def canonical_space_id(contract: dict[str, Any]) -> str:
    encoded = json.dumps(contract, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def parse_contract_component(value: str) -> dict[str, Any]:
    parts = value.split("=", 3)
    if len(parts) != 4:
        raise ValueError("component must be NAME=SPACE_ID=DIMENSIONS=WEIGHT")
    name, space_id, dimensions_text, weight_text = parts
    if (
        not name
        or any(character not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for character in name)
        or not is_sha256(space_id)
    ):
        raise ValueError("component has an invalid name or embedding-space ID")
    try:
        dimensions = int(dimensions_text)
        weight = float(weight_text)
    except ValueError as error:
        raise ValueError("component dimensions and weight must be numeric") from error
    if dimensions <= 0 or not math.isfinite(weight) or weight <= 0:
        raise ValueError("component dimensions and weight must be positive")
    return {
        "name": name,
        "embedding_space_id": space_id,
        "dimensions": dimensions,
        "weight": weight,
    }


def build_contract(specifications: list[str]) -> dict[str, Any]:
    if len(specifications) < 2:
        raise ValueError("concatenation requires at least two components")
    components = [parse_contract_component(value) for value in specifications]
    names = [component["name"] for component in components]
    if len(set(names)) != len(names):
        raise ValueError("component names must be unique")
    total_weight = sum(component.pop("weight") for component in components)
    # Weight fractions are the actual operation. Canonicalizing proportional
    # inputs avoids assigning different space IDs to mathematically identical
    # combinations such as 1:1 and 2:2.
    raw_weights = [parse_contract_component(value)["weight"] for value in specifications]
    for component, weight in zip(components, raw_weights):
        component["weight_fraction"] = weight / total_weight
    return {
        "schema": CONTRACT_SCHEMA,
        "transform": TRANSFORM,
        "output_dimensions": sum(component["dimensions"] for component in components),
        "components": components,
    }


def validate_contract(contract: object) -> dict[str, Any]:
    if not isinstance(contract, dict) or frozenset(contract) != {
        "schema",
        "transform",
        "output_dimensions",
        "components",
    }:
        raise ValueError("invalid concatenation contract fields")
    if contract["schema"] != CONTRACT_SCHEMA or contract["transform"] != TRANSFORM:
        raise ValueError("unsupported concatenation contract")
    components = contract["components"]
    if not isinstance(components, list) or len(components) < 2:
        raise ValueError("concatenation contract requires at least two components")
    names = set()
    dimensions = 0
    weight_sum = 0.0
    for component in components:
        if not isinstance(component, dict) or frozenset(component) != {
            "name",
            "embedding_space_id",
            "dimensions",
            "weight_fraction",
        }:
            raise ValueError("invalid concatenation component fields")
        name = component["name"]
        dims = component["dimensions"]
        weight = component["weight_fraction"]
        if (
            not isinstance(name, str)
            or not name
            or name in names
            or not is_sha256(component["embedding_space_id"])
            or isinstance(dims, bool)
            or not isinstance(dims, int)
            or dims <= 0
            or isinstance(weight, bool)
            or not isinstance(weight, (int, float))
            or not math.isfinite(weight)
            or weight <= 0
        ):
            raise ValueError("invalid concatenation component")
        names.add(name)
        dimensions += dims
        weight_sum += float(weight)
    if contract["output_dimensions"] != dimensions:
        raise ValueError("concatenation output dimensions do not match components")
    if not math.isclose(weight_sum, 1.0, rel_tol=0.0, abs_tol=1e-12):
        raise ValueError("concatenation component weights must sum to one")
    return contract


def write_contract(path: Path, contract: dict[str, Any]) -> None:
    validate_contract(contract)
    envelope = {
        "contract": contract,
        "embedding_space_id": canonical_space_id(contract),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(envelope, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def read_contract(path: Path) -> tuple[dict[str, Any], str]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict) or frozenset(payload) != {
        "contract",
        "embedding_space_id",
    }:
        raise ValueError("contract envelope has invalid fields")
    contract = validate_contract(payload["contract"])
    space_id = payload["embedding_space_id"]
    if space_id != canonical_space_id(contract):
        raise ValueError("concatenation embedding-space ID does not match contract")
    return contract, space_id


def parse_inputs(values: list[str], contract: dict[str, Any]) -> list[Path]:
    supplied = {}
    for value in values:
        name, separator, path = value.partition("=")
        if not separator or not name or not path or name in supplied:
            raise ValueError("input must be a unique NAME=PATH")
        supplied[name] = Path(path)
    expected = [component["name"] for component in contract["components"]]
    if set(supplied) != set(expected):
        raise ValueError("input names must exactly match the contract components")
    paths = [supplied[name] for name in expected]
    missing = [str(path) for path in paths if not path.is_file()]
    if missing:
        raise ValueError("missing component inputs: " + ", ".join(missing))
    return paths


def normalize(matrix: np.ndarray) -> np.ndarray:
    matrix64 = np.asarray(matrix, dtype=np.float64)
    if matrix64.ndim != 2 or not np.all(np.isfinite(matrix64)):
        raise ValueError("component vectors must be a finite rank-2 matrix")
    norms = np.sqrt(np.sum(matrix64 * matrix64, axis=1, keepdims=True))
    if np.any(norms <= 0.0) or not np.all(np.isfinite(norms)):
        raise ValueError("component vectors contain a zero-norm row")
    return matrix64 / norms


def concatenate_matrices(
    matrices: list[np.ndarray], contract: dict[str, Any]
) -> np.ndarray:
    components = contract["components"]
    if len(matrices) != len(components):
        raise ValueError("matrix count does not match contract")
    row_count = matrices[0].shape[0] if matrices else 0
    scaled = []
    for matrix, component in zip(matrices, components):
        if matrix.ndim != 2 or matrix.shape != (row_count, component["dimensions"]):
            raise ValueError("component matrix shape does not match contract")
        scaled.append(normalize(matrix) * math.sqrt(component["weight_fraction"]))
    return normalize(np.concatenate(scaled, axis=1)).astype("<f4")


def read_f32(path: Path, dimensions: int) -> np.ndarray:
    raw = np.fromfile(path, dtype="<f4")
    if not len(raw) or len(raw) % dimensions:
        raise ValueError(f"{path} is not an exact nonempty {dimensions}D f32 matrix")
    return raw.reshape(-1, dimensions)


def corpus(args: argparse.Namespace) -> None:
    contract, _space_id = read_contract(args.contract)
    paths = parse_inputs(args.input, contract)
    matrices = [
        read_f32(path, component["dimensions"])
        for path, component in zip(paths, contract["components"])
    ]
    rows = {matrix.shape[0] for matrix in matrices}
    if len(rows) != 1:
        raise ValueError("corpus components must have the same row count")
    output = concatenate_matrices(matrices, contract)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output.tofile(args.output)


def read_query_rows(path: Path) -> list[dict[str, Any]]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        if not isinstance(row, dict):
            raise ValueError(f"{path}:{line_number}: query row must be an object")
        rows.append(row)
    if not rows:
        raise ValueError(f"{path}: query file is empty")
    return rows


def query_vector(row: dict[str, Any], dimensions: int, context: str) -> np.ndarray:
    vector = row.get("embedding")
    if not isinstance(vector, list) or len(vector) != dimensions:
        raise ValueError(f"{context}: embedding dimensions do not match contract")
    return np.asarray([vector], dtype=np.float32)


def queries(args: argparse.Namespace) -> None:
    contract, _space_id = read_contract(args.contract)
    paths = parse_inputs(args.input, contract)
    sources = [read_query_rows(path) for path in paths]
    row_counts = {len(rows) for rows in sources}
    if len(row_counts) != 1:
        raise ValueError("query components must have the same row count")

    output_rows = []
    for row_index, source_rows in enumerate(zip(*sources), 1):
        first = source_rows[0]
        identity = (
            first.get("id"),
            first.get(PROMPT_DIGEST),
            first.get("planner"),
        )
        if (
            not isinstance(identity[0], str)
            or not is_sha256(identity[1])
            or any(
                (row.get("id"), row.get(PROMPT_DIGEST), row.get("planner")) != identity
                for row in source_rows[1:]
            )
        ):
            raise ValueError(f"query row {row_index}: component metadata differs")
        if identity[2] not in (None, QUERY_SEGMENT_PLANNER):
            raise ValueError(f"query row {row_index}: unsupported segment planner")
        whole = concatenate_matrices(
            [
                query_vector(row, component["dimensions"], f"query row {row_index}")
                for row, component in zip(source_rows, contract["components"])
            ],
            contract,
        )[0]
        output = {
            "id": identity[0],
            PROMPT_DIGEST: identity[1],
            "embedding": whole.tolist(),
        }
        segment_lists = [row.get("segments", []) for row in source_rows]
        if any(not isinstance(segments, list) for segments in segment_lists):
            raise ValueError(f"query row {row_index}: segments must be arrays")
        if len({len(segments) for segments in segment_lists}) != 1:
            raise ValueError(f"query row {row_index}: component segment counts differ")
        if segment_lists[0]:
            if identity[2] != QUERY_SEGMENT_PLANNER:
                raise ValueError(f"query row {row_index}: unsupported segment planner")
            output["planner"] = identity[2]
            output["segments"] = []
            for segment_index, source_segments in enumerate(zip(*segment_lists), 1):
                text = source_segments[0].get("text")
                if not isinstance(text, str) or any(
                    segment.get("text") != text for segment in source_segments[1:]
                ):
                    raise ValueError(
                        f"query row {row_index} segment {segment_index}: text differs"
                    )
                combined = concatenate_matrices(
                    [
                        query_vector(
                            segment,
                            component["dimensions"],
                            f"query row {row_index} segment {segment_index}",
                        )
                        for segment, component in zip(
                            source_segments, contract["components"]
                        )
                    ],
                    contract,
                )[0]
                output["segments"].append(
                    {"text": text, "embedding": combined.tolist()}
                )
        output_rows.append(output)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as destination:
        for row in output_rows:
            destination.write(json.dumps(row, separators=(",", ":")) + "\n")


def contract_command(args: argparse.Namespace) -> None:
    write_contract(args.output, build_contract(args.component))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    contract_parser = commands.add_parser("contract")
    contract_parser.add_argument("--component", action="append", required=True)
    contract_parser.add_argument("--output", type=Path, required=True)
    contract_parser.set_defaults(run=contract_command)
    for name, function in (("corpus", corpus), ("queries", queries)):
        command = commands.add_parser(name)
        command.add_argument("--contract", type=Path, required=True)
        command.add_argument("--input", action="append", required=True)
        command.add_argument("--output", type=Path, required=True)
        command.set_defaults(run=function)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
