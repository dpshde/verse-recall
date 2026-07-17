#!/usr/bin/env python3
"""Strict, dependency-free validation for semantic embedding-space contracts."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any


SPACE_CONTRACT_SCHEMA = "verse-recall-embedding-space-v1"
OUTPUT_NORMALIZATION = "l2-f32-v1"
SUPPORTED_POOLING = frozenset({"cls", "mean"})
PRE_TRUNCATE_LAYER_NORM = "per-row-epsilon-1e-5-v1"

_REQUIRED_CONTRACT_KEYS = frozenset(
    {
        "schema",
        "encoder_assets",
        "tokenizer_sha256",
        "pooling",
        "max_length",
        "query_prefix",
        "corpus_prefix",
        "projection_sha256",
        "output_normalization",
    }
)
_OPTIONAL_CONTRACT_KEYS = frozenset(
    {
        "truncate_dims",
        "pre_truncate_layer_norm",
        "query_adapter_sha256",
        "query_adapter_application",
    }
)


def _is_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def contract_space_id(contract: dict[str, Any]) -> str:
    """Return the content identity used by LVEC v2 packs."""

    canonical = json.dumps(contract, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def validate_contract(contract: object) -> dict[str, Any]:
    """Validate one complete v1 contract and return it unchanged."""

    if not isinstance(contract, dict):
        raise ValueError("embedding-space contract must be a JSON object")
    keys = frozenset(contract)
    missing = _REQUIRED_CONTRACT_KEYS - keys
    unknown = keys - _REQUIRED_CONTRACT_KEYS - _OPTIONAL_CONTRACT_KEYS
    if missing:
        raise ValueError(
            "embedding-space contract is missing fields: " + ", ".join(sorted(missing))
        )
    if unknown:
        raise ValueError(
            "embedding-space contract has unsupported fields: "
            + ", ".join(sorted(unknown))
        )
    if contract["schema"] != SPACE_CONTRACT_SCHEMA:
        raise ValueError("unsupported embedding-space contract schema")
    if contract["output_normalization"] != OUTPUT_NORMALIZATION:
        raise ValueError("unsupported embedding output normalization")
    if contract["pooling"] not in SUPPORTED_POOLING:
        raise ValueError("unsupported embedding pooling mode")
    max_length = contract["max_length"]
    if isinstance(max_length, bool) or not isinstance(max_length, int) or max_length <= 0:
        raise ValueError("embedding max_length must be a positive integer")
    for field in ("query_prefix", "corpus_prefix"):
        if not isinstance(contract[field], str):
            raise ValueError(f"embedding {field} must be a string")

    assets = contract["encoder_assets"]
    if not isinstance(assets, list) or not assets:
        raise ValueError("embedding encoder_assets must be a non-empty array")
    names: set[str] = set()
    onnx_count = 0
    for index, asset in enumerate(assets, 1):
        if not isinstance(asset, dict) or frozenset(asset) != {"name", "sha256"}:
            raise ValueError(f"encoder asset {index} must contain only name and sha256")
        name = asset["name"]
        if (
            not isinstance(name, str)
            or not name
            or Path(name).name != name
            or name in names
        ):
            raise ValueError(f"encoder asset {index} has an invalid name")
        if not _is_sha256(asset["sha256"]):
            raise ValueError(f"encoder asset {index} has an invalid sha256")
        names.add(name)
        onnx_count += int(name.endswith(".onnx"))
    if onnx_count != 1:
        raise ValueError("embedding-space contract must identify exactly one ONNX graph")
    if not _is_sha256(contract["tokenizer_sha256"]):
        raise ValueError("embedding tokenizer_sha256 is invalid")

    projection = contract["projection_sha256"]
    if projection is not None and not _is_sha256(projection):
        raise ValueError("embedding projection_sha256 is invalid")
    truncate_dims = contract.get("truncate_dims")
    if truncate_dims is not None and (
        isinstance(truncate_dims, bool)
        or not isinstance(truncate_dims, int)
        or truncate_dims <= 0
    ):
        raise ValueError("embedding truncate_dims must be a positive integer")
    if projection is not None and truncate_dims is not None:
        raise ValueError("embedding projection and truncation are mutually exclusive")

    pre_normalization = contract.get("pre_truncate_layer_norm")
    if pre_normalization is not None:
        if pre_normalization != PRE_TRUNCATE_LAYER_NORM:
            raise ValueError("unsupported pre-truncation normalization")
        if truncate_dims is None:
            raise ValueError("pre-truncation normalization requires truncate_dims")

    adapter_sha = contract.get("query_adapter_sha256")
    adapter_application = contract.get("query_adapter_application")
    if (adapter_sha is None) != (adapter_application is None):
        raise ValueError("query-adapter identity and application must appear together")
    if adapter_sha is not None:
        if not _is_sha256(adapter_sha):
            raise ValueError("embedding query_adapter_sha256 is invalid")
        if adapter_application != "post-encoder-query-only-v1":
            raise ValueError("unsupported query-adapter application contract")
    return contract


def validate_envelope(payload: object) -> tuple[dict[str, Any], str]:
    """Validate a content-addressed contract envelope."""

    if not isinstance(payload, dict) or frozenset(payload) != {
        "contract",
        "embedding_space_id",
    }:
        raise ValueError(
            "embedding-space file must contain exactly contract and embedding_space_id"
        )
    contract = validate_contract(payload["contract"])
    declared = payload["embedding_space_id"]
    if not _is_sha256(declared):
        raise ValueError("declared embedding-space ID is invalid")
    actual = contract_space_id(contract)
    if declared != actual:
        raise ValueError("declared embedding-space ID does not match its contract")
    return contract, actual
