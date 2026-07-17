#!/usr/bin/env python3
"""Retire one revealed holdout JSONL into a separate regression JSONL.

The command deliberately reports only record counts and file paths.  It never
logs prompt, gold-reference, family, or other row content.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import stat
import sys
from typing import Any, Iterable


HOLDOUT_SPLIT_RE = re.compile(r"holdout-([A-Za-z0-9][A-Za-z0-9._-]*)\Z")


class RetirementError(ValueError):
    """A safe-to-display holdout-retirement failure."""


class _DuplicateKeyError(ValueError):
    pass


@dataclass(frozen=True)
class RetirementResult:
    records: int
    source_split: str
    destination_split: str
    destination: Path


def partial_path_for(destination: Path) -> Path:
    """Return the deterministic same-directory partial path."""

    return destination.with_name(f".{destination.name}.retire.tmp")


def _object_without_duplicate_keys(pairs: Iterable[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise _DuplicateKeyError("duplicate object key")
        result[key] = value
    return result


def _reject_non_json_constant(_value: str) -> None:
    raise ValueError("non-JSON numeric constant")


def _parse_object(line: str, line_number: int) -> dict[str, Any]:
    try:
        value = json.loads(
            line,
            object_pairs_hook=_object_without_duplicate_keys,
            parse_constant=_reject_non_json_constant,
        )
    except (json.JSONDecodeError, _DuplicateKeyError, ValueError, TypeError) as error:
        raise RetirementError(
            f"source line {line_number} is not valid strict JSON"
        ) from error
    if not isinstance(value, dict):
        raise RetirementError(f"source line {line_number} is not a JSON object")
    return value


def _line_ending(line: str) -> tuple[str, str]:
    for ending in ("\r\n", "\n", "\r"):
        if line.endswith(ending):
            return line[: -len(ending)], ending
    return line, ""


def _transform_rows(source: Path) -> tuple[list[str], int, str, str]:
    rendered: list[str] = []
    source_split: str | None = None
    destination_split: str | None = None
    records = 0

    try:
        with source.open("r", encoding="utf-8", newline="") as input_file:
            lines = list(input_file)
    except (OSError, UnicodeError) as error:
        raise RetirementError("source could not be read as UTF-8 JSONL") from error

    for line_number, original_line in enumerate(lines, 1):
        body, ending = _line_ending(original_line)
        if not body.strip() or body.lstrip().startswith("#"):
            rendered.append(original_line)
            continue

        row = _parse_object(body, line_number)
        split = row.get("split")
        if not isinstance(split, str):
            raise RetirementError(
                f"source line {line_number} requires a string split"
            )
        match = HOLDOUT_SPLIT_RE.fullmatch(split)
        if match is None:
            raise RetirementError(
                f"source line {line_number} does not use a valid holdout-* split"
            )
        if source_split is None:
            source_split = split
            destination_split = f"regression-{match.group(1)}"
        elif split != source_split:
            raise RetirementError("source contains more than one holdout split")

        transformed = dict(row)
        transformed["split"] = destination_split
        serialized = json.dumps(
            transformed,
            ensure_ascii=True,
            allow_nan=False,
            separators=(",", ":"),
        )
        reparsed = _parse_object(serialized, line_number)
        if reparsed != transformed or (
            "prompt" in row and reparsed.get("prompt") != row["prompt"]
        ):
            raise RetirementError(
                f"source line {line_number} could not be transformed losslessly"
            )
        rendered.append(serialized + ending)
        records += 1

    if records == 0 or source_split is None or destination_split is None:
        raise RetirementError("source contains no JSON records")
    return rendered, records, source_split, destination_split


def _is_nonempty_regular_file(path: Path) -> bool:
    try:
        metadata = path.stat()
    except OSError:
        return False
    return stat.S_ISREG(metadata.st_mode) and metadata.st_size > 0


def _path_exists(path: Path) -> bool:
    """Include dangling symlinks when detecting an unsafe existing state."""

    return os.path.lexists(path)


def _resolved_destination(path: Path) -> Path:
    return path.parent.resolve() / path.name


def _validate_preconditions(
    source: Path, destination: Path, first_shot_report: Path
) -> tuple[Path, Path]:
    if first_shot_report.suffix.lower() != ".md":
        raise RetirementError("first-shot report path must end in .md")
    json_report = first_shot_report.with_suffix(".json")
    if not _is_nonempty_regular_file(first_shot_report):
        raise RetirementError("first-shot Markdown report is missing or empty")
    if not _is_nonempty_regular_file(json_report):
        raise RetirementError("paired first-shot JSON report is missing or empty")

    if not _is_nonempty_regular_file(source):
        raise RetirementError("source JSONL is missing, empty, or not a regular file")
    if not destination.parent.is_dir():
        raise RetirementError("destination parent directory does not exist")
    if source.resolve() == _resolved_destination(destination):
        raise RetirementError("destination must be separate from the sealed source")
    if _path_exists(destination):
        raise RetirementError("destination already exists")

    partial = partial_path_for(destination)
    if _path_exists(partial):
        raise RetirementError("a partial retirement state already exists")
    return json_report, partial


def _fsync_directory(directory: Path) -> None:
    descriptor = os.open(directory, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def retire_holdout(
    source: Path, destination: Path, first_shot_report: Path
) -> RetirementResult:
    """Validate, transform, fsync, and atomically publish a regression JSONL."""

    source = Path(source)
    destination = Path(destination)
    first_shot_report = Path(first_shot_report)
    _json_report, partial = _validate_preconditions(
        source, destination, first_shot_report
    )
    rendered, records, source_split, destination_split = _transform_rows(source)

    descriptor: int | None = None
    partial_created = False
    published = False
    try:
        descriptor = os.open(
            partial,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL,
            0o600,
        )
        partial_created = True
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="") as output_file:
            descriptor = None
            output_file.writelines(rendered)
            output_file.flush()
            os.fsync(output_file.fileno())

        # Refuse a destination that appeared after preflight.  os.replace is
        # the atomic same-directory commit; the deterministic partial makes an
        # interrupted pre-commit run visible and fail-closed on the next run.
        if _path_exists(destination):
            raise RetirementError("destination appeared during retirement")
        os.replace(partial, destination)
        published = True
        partial_created = False
        _fsync_directory(destination.parent)
    except RetirementError:
        raise
    except (OSError, UnicodeError) as error:
        raise RetirementError("retirement could not be committed atomically") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
        if partial_created and not published:
            try:
                os.unlink(partial)
            except OSError:
                # Leave an unremovable partial visible. The next run refuses
                # it instead of silently treating the prior failure as clean.
                pass

    return RetirementResult(
        records=records,
        source_split=source_split,
        destination_split=destination_split,
        destination=destination,
    )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Retire a revealed holdout into a separate regression JSONL"
    )
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument(
        "--first-shot-report",
        type=Path,
        required=True,
        help="Markdown report path; the paired .json file is required",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        result = retire_holdout(
            args.source, args.destination, args.first_shot_report
        )
    except RetirementError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    print(f"retired {result.records} records to {result.destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
