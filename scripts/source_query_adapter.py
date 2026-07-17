#!/usr/bin/env python3
"""Portable NumPy runtime for compact residual query adapters.

The binary format deliberately contains only an architecture contract and
little-endian float32 parameters.  It does not depend on PyTorch, pickle, or
host endianness.  Artifacts are named by the SHA-256 digest of their exact
bytes, making the filename a content address rather than a mutable model name.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
from pathlib import Path
import re
import struct

import numpy as np


MAGIC = b"VRNQAD01"
FORMAT_VERSION = 1
ACTIVATION_TANH_GELU = 1
FLAG_NORMALIZE_INPUT = 1 << 0
FLAG_NORMALIZE_OUTPUT = 1 << 1
REQUIRED_FLAGS = FLAG_NORMALIZE_INPUT | FLAG_NORMALIZE_OUTPUT
HEADER = struct.Struct("<8sIIIIIf")
CONTENT_ADDRESS_RE = re.compile(
    r"^(?P<prefix>[A-Za-z0-9][A-Za-z0-9._-]*)-"
    r"(?P<digest>[0-9a-f]{64})\.vrqa$"
)
_GELU_FACTOR = np.float32(0.7978845608028654)  # sqrt(2 / pi)
_GELU_CUBIC = np.float32(0.044715)


@dataclass(frozen=True)
class AdapterParameters:
    """Parameters for ``normalize(x + scale * up(gelu(down(x))))``."""

    down_weight: np.ndarray
    down_bias: np.ndarray
    up_weight: np.ndarray
    up_bias: np.ndarray
    residual_scale: float = 1.0

    @property
    def input_dim(self) -> int:
        return int(self.down_weight.shape[0])

    @property
    def hidden_dim(self) -> int:
        return int(self.down_weight.shape[1])


def _as_finite_f32(name: str, value: np.ndarray) -> np.ndarray:
    result = np.asarray(value, dtype=np.float32, order="C")
    if not np.all(np.isfinite(result)):
        raise ValueError(f"{name} contains nonfinite values")
    return result


def validate_parameters(parameters: AdapterParameters) -> AdapterParameters:
    down_weight = _as_finite_f32("down_weight", parameters.down_weight)
    down_bias = _as_finite_f32("down_bias", parameters.down_bias)
    up_weight = _as_finite_f32("up_weight", parameters.up_weight)
    up_bias = _as_finite_f32("up_bias", parameters.up_bias)
    if down_weight.ndim != 2 or not all(down_weight.shape):
        raise ValueError("down_weight must be a non-empty 2D matrix")
    input_dim, hidden_dim = down_weight.shape
    if down_bias.shape != (hidden_dim,):
        raise ValueError("down_bias shape does not match down_weight")
    if up_weight.shape != (hidden_dim, input_dim):
        raise ValueError("up_weight shape does not match down_weight")
    if up_bias.shape != (input_dim,):
        raise ValueError("up_bias shape does not match down_weight")
    residual_scale = float(parameters.residual_scale)
    if not np.isfinite(residual_scale) or residual_scale <= 0.0:
        raise ValueError("residual_scale must be finite and positive")
    return AdapterParameters(
        down_weight=down_weight,
        down_bias=down_bias,
        up_weight=up_weight,
        up_bias=up_bias,
        residual_scale=residual_scale,
    )


def tanh_gelu(value: np.ndarray) -> np.ndarray:
    """Deterministic tanh GELU approximation shared with the trainer."""

    value = np.asarray(value, dtype=np.float32)
    inner = _GELU_FACTOR * (value + _GELU_CUBIC * value * value * value)
    return np.float32(0.5) * value * (np.float32(1.0) + np.tanh(inner))


def _normalize_rows(vectors: np.ndarray, label: str) -> np.ndarray:
    vectors = _as_finite_f32(label, vectors)
    norms = np.linalg.norm(vectors, axis=1, keepdims=True)
    if np.any(~np.isfinite(norms)) or np.any(norms <= np.float32(1e-12)):
        raise ValueError(f"{label} contains a zero-norm row")
    return np.asarray(vectors / norms, dtype=np.float32, order="C")


def checked_matmul(left: np.ndarray, right: np.ndarray, label: str) -> np.ndarray:
    """Multiply arrays and validate the result instead of trusting BLAS flags.

    NumPy 2.0 linked against Apple's Accelerate can leave IEEE status flags set
    after a finite matrix product and consequently emit false divide/overflow
    warnings.  Suppressing those flags is safe here because every operand is
    validated before use and the complete result is checked immediately.
    """

    with np.errstate(divide="ignore", over="ignore", invalid="ignore"):
        result = np.matmul(left, right)
    if not np.all(np.isfinite(result)):
        raise ValueError(f"{label} produced nonfinite values")
    return np.asarray(result, dtype=np.float32, order="C")


def apply_adapter(
    vectors: np.ndarray, parameters: AdapterParameters, batch_size: int = 4096
) -> np.ndarray:
    """Apply an adapter using NumPy only and return unit-normalized float32.

    Both a single ``[D]`` vector and a matrix ``[N, D]`` are accepted.  Input
    normalization is part of the format contract, so callers cannot silently
    change adapter behavior by varying encoder post-processing.
    """

    if batch_size <= 0:
        raise ValueError("batch_size must be positive")
    parameters = validate_parameters(parameters)
    original = np.asarray(vectors)
    was_vector = original.ndim == 1
    matrix = original.reshape(1, -1) if was_vector else original
    if matrix.ndim != 2 or matrix.shape[1] != parameters.input_dim:
        raise ValueError(
            f"adapter expects [N, {parameters.input_dim}] or "
            f"[{parameters.input_dim}] input"
        )
    matrix = _normalize_rows(matrix, "adapter input")
    output = np.empty_like(matrix, dtype=np.float32)
    for start in range(0, len(matrix), batch_size):
        source = matrix[start : start + batch_size]
        hidden = tanh_gelu(
            checked_matmul(
                source, parameters.down_weight, "adapter down projection"
            )
            + parameters.down_bias
        )
        residual = (
            checked_matmul(hidden, parameters.up_weight, "adapter up projection")
            + parameters.up_bias
        )
        adapted = source + np.float32(parameters.residual_scale) * residual
        output[start : start + len(source)] = _normalize_rows(
            adapted, "adapter output"
        )
    return output[0] if was_vector else output


def serialize_adapter(parameters: AdapterParameters) -> bytes:
    parameters = validate_parameters(parameters)
    header = HEADER.pack(
        MAGIC,
        FORMAT_VERSION,
        parameters.input_dim,
        parameters.hidden_dim,
        ACTIVATION_TANH_GELU,
        REQUIRED_FLAGS,
        parameters.residual_scale,
    )
    arrays = (
        parameters.down_weight,
        parameters.down_bias,
        parameters.up_weight,
        parameters.up_bias,
    )
    return header + b"".join(
        np.asarray(array, dtype="<f4", order="C").tobytes(order="C")
        for array in arrays
    )


def deserialize_adapter(data: bytes) -> AdapterParameters:
    if len(data) < HEADER.size:
        raise ValueError("query adapter artifact is shorter than its header")
    (
        magic,
        version,
        input_dim,
        hidden_dim,
        activation,
        flags,
        residual_scale,
    ) = HEADER.unpack_from(data)
    if magic != MAGIC:
        raise ValueError("query adapter artifact has an invalid magic number")
    if version != FORMAT_VERSION:
        raise ValueError(f"unsupported query adapter version {version}")
    if input_dim <= 0 or hidden_dim <= 0:
        raise ValueError("query adapter dimensions must be positive")
    if activation != ACTIVATION_TANH_GELU:
        raise ValueError(f"unsupported query adapter activation {activation}")
    if flags != REQUIRED_FLAGS:
        raise ValueError(f"unsupported query adapter flags {flags:#x}")
    float_count = input_dim * hidden_dim + hidden_dim + hidden_dim * input_dim + input_dim
    expected_size = HEADER.size + float_count * np.dtype("<f4").itemsize
    if len(data) != expected_size:
        raise ValueError(
            f"query adapter artifact has {len(data)} bytes; expected {expected_size}"
        )
    values = np.frombuffer(data, dtype="<f4", offset=HEADER.size)
    offset = 0
    down_weight = values[offset : offset + input_dim * hidden_dim].reshape(
        input_dim, hidden_dim
    )
    offset += input_dim * hidden_dim
    down_bias = values[offset : offset + hidden_dim]
    offset += hidden_dim
    up_weight = values[offset : offset + hidden_dim * input_dim].reshape(
        hidden_dim, input_dim
    )
    offset += hidden_dim * input_dim
    up_bias = values[offset : offset + input_dim]
    return validate_parameters(
        AdapterParameters(
            down_weight=down_weight.copy(),
            down_bias=down_bias.copy(),
            up_weight=up_weight.copy(),
            up_bias=up_bias.copy(),
            residual_scale=float(residual_scale),
        )
    )


def content_sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save_content_addressed(
    output_dir: Path,
    parameters: AdapterParameters,
    prefix: str = "source-query-adapter",
) -> Path:
    """Write an immutable ``<prefix>-<sha256>.vrqa`` artifact."""

    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", prefix):
        raise ValueError("artifact prefix contains unsupported characters")
    data = serialize_adapter(parameters)
    digest = content_sha256(data)
    output_dir.mkdir(parents=True, exist_ok=True)
    destination = output_dir / f"{prefix}-{digest}.vrqa"
    if destination.exists():
        if destination.read_bytes() != data:
            raise ValueError(f"content-address collision at {destination}")
        return destination
    temporary = output_dir / f".{destination.name}.tmp"
    temporary.write_bytes(data)
    temporary.replace(destination)
    return destination


def load_content_addressed(path: Path) -> AdapterParameters:
    match = CONTENT_ADDRESS_RE.fullmatch(path.name)
    if match is None:
        raise ValueError("query adapter filename is not content-addressed")
    data = path.read_bytes()
    actual = content_sha256(data)
    if actual != match.group("digest"):
        raise ValueError(
            f"query adapter content digest mismatch: filename={match.group('digest')} "
            f"actual={actual}"
        )
    return deserialize_adapter(data)
