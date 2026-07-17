#!/usr/bin/env python3
"""Serve the local manual-search UI against the release recall CLI."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import threading
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

try:
    from embedding_space_contract import (
        PRE_TRUNCATE_LAYER_NORM,
        validate_envelope,
    )
except ModuleNotFoundError:  # Imported as a repository module in unit tests.
    from scripts.embedding_space_contract import (
        PRE_TRUNCATE_LAYER_NORM,
        validate_envelope,
    )

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "examples" / "web" / "index.html"
CLI = ROOT / "target" / "release" / "recall-cli"
PACKS = (
    ROOT / "data" / "bsb-full.pack",
    ROOT / "data" / "bsb-expand.pack",
    ROOT / "data" / "recall-overlay.pack",
)
SEMANTIC_PACK: Path | None = None
SEMANTIC_ENCODER = None
SEMANTIC_QUERY_ADAPTER = None
SEMANTIC_QUERY_PREFIX = ""
SEMANTIC_SPACE_ID = ""
SEMANTIC_LOCK = threading.Lock()


class SearchHandler(BaseHTTPRequestHandler):
    server_version = "VerseRecallUI/1"

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
        parsed = urlparse(self.path)
        if parsed.path == "/":
            self._send(HTTPStatus.OK, INDEX.read_bytes(), "text/html; charset=utf-8")
            return
        if parsed.path == "/api/search":
            self._search(parse_qs(parsed.query))
            return
        self._json(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def _search(self, params: dict[str, list[str]]) -> None:
        query = params.get("q", [""])[0].strip()
        if not query or len(query) > 500:
            self._json(HTTPStatus.BAD_REQUEST, {"error": "query must contain 1–500 characters"})
            return
        try:
            top_k = int(params.get("top_k", ["5"])[0])
        except ValueError:
            top_k = 5
        top_k = min(max(top_k, 1), 20)
        book_hint = params.get("book_hint", [""])[0].strip()

        command = [
            str(CLI),
            "search",
            "--pack",
            str(PACKS[0]),
            "--overlay",
            str(PACKS[1]),
            "--overlay",
            str(PACKS[2]),
            "--top-k",
            str(top_k),
            "--json",
        ]
        embedding_path = None
        if SEMANTIC_ENCODER is not None and SEMANTIC_PACK is not None:
            # The host owns inference; recall-core still receives only a
            # validated vector and canonical query-only clause plan.
            from build_semantic_vectors import encode_query_records

            with SEMANTIC_LOCK:
                record = encode_query_records(
                    [{"id": "manual", "prompt": query}],
                    SEMANTIC_ENCODER,
                    SEMANTIC_QUERY_PREFIX,
                    1,
                    segment_embeddings=True,
                    query_adapter=SEMANTIC_QUERY_ADAPTER,
                )[0]
            temporary = tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                prefix="verse-recall-query-",
                suffix=".jsonl",
                delete=False,
            )
            try:
                temporary.write(json.dumps(record, separators=(",", ":")) + "\n")
            finally:
                temporary.close()
            embedding_path = Path(temporary.name)
            command[command.index("--top-k"):command.index("--top-k")] = [
                "--overlay",
                str(SEMANTIC_PACK),
                "--query-embeddings",
                str(embedding_path),
                "--embedding-space-id",
                SEMANTIC_SPACE_ID,
            ]
        if book_hint:
            command.extend(("--book-hint", book_hint[:80]))
        command.append(query)
        try:
            completed = subprocess.run(
                command,
                cwd=ROOT,
                check=True,
                capture_output=True,
                text=True,
                timeout=15,
            )
            payload = json.loads(completed.stdout)
            payload["engine"] = (
                "BSB + expansion + KJV/knowledge + Arctic-M256 semantic"
                + (" + source-trained query adapter" if SEMANTIC_QUERY_ADAPTER is not None else "")
                if SEMANTIC_ENCODER is not None
                else "BSB + expansion + KJV/knowledge overlay"
            )
            self._json(HTTPStatus.OK, payload)
        except (subprocess.SubprocessError, json.JSONDecodeError) as error:
            self._json(HTTPStatus.INTERNAL_SERVER_ERROR, {"error": str(error)})
        finally:
            if embedding_path is not None:
                embedding_path.unlink(missing_ok=True)

    def _json(self, status: HTTPStatus, payload: object) -> None:
        self._send(status, json.dumps(payload).encode(), "application/json")

    def _send(self, status: HTTPStatus, body: bytes, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:
        sys.stderr.write(f"[search-ui] {format % args}\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--semantic-pack", type=Path)
    parser.add_argument("--model-dir", type=Path)
    parser.add_argument("--space-contract", type=Path)
    parser.add_argument(
        "--query-adapter",
        type=Path,
        help="VRQADP01 query-only adapter required by the embedding-space contract",
    )
    parser.add_argument("--port", type=int, default=4173)
    args = parser.parse_args()
    semantic_values = (args.semantic_pack, args.model_dir, args.space_contract)
    if any(semantic_values) and not all(semantic_values):
        raise SystemExit(
            "--semantic-pack, --model-dir, and --space-contract must be supplied together"
        )
    if args.query_adapter is not None and not all(semantic_values):
        raise SystemExit("--query-adapter requires the complete semantic configuration")

    global SEMANTIC_PACK, SEMANTIC_ENCODER, SEMANTIC_QUERY_ADAPTER
    global SEMANTIC_QUERY_PREFIX, SEMANTIC_SPACE_ID
    if all(semantic_values):
        from build_semantic_vectors import (
            Encoder,
            load_query_adapter,
            sha256_file,
        )

        contract_file = json.loads(args.space_contract.read_text(encoding="utf-8"))
        try:
            contract, semantic_space_id = validate_envelope(contract_file)
        except ValueError as error:
            raise SystemExit(str(error)) from error
        assets = contract["encoder_assets"]
        onnx_assets = [asset for asset in assets if asset.get("name", "").endswith(".onnx")]
        for asset in assets:
            path = args.model_dir / asset["name"]
            if not path.is_file() or sha256_file(path) != asset.get("sha256"):
                raise SystemExit(f"model asset does not match space contract: {path}")
        tokenizer = args.model_dir / "tokenizer.json"
        if (
            not tokenizer.is_file()
            or sha256_file(tokenizer) != contract.get("tokenizer_sha256")
        ):
            raise SystemExit("tokenizer does not match space contract")
        if contract.get("projection_sha256") is not None:
            raise SystemExit("manual UI does not yet accept projected embedding spaces")
        expected_adapter_sha = contract.get("query_adapter_sha256")
        if (expected_adapter_sha is None) != (args.query_adapter is None):
            raise SystemExit(
                "query adapter presence does not match the embedding-space contract"
            )
        if args.query_adapter is not None:
            if (
                contract.get("query_adapter_application")
                != "post-encoder-query-only-v1"
            ):
                raise SystemExit("unsupported query-adapter application contract")
            if (
                not args.query_adapter.is_file()
                or sha256_file(args.query_adapter) != expected_adapter_sha
            ):
                raise SystemExit("query adapter does not match space contract")
            SEMANTIC_QUERY_ADAPTER = load_query_adapter(args.query_adapter)
        SEMANTIC_ENCODER = Encoder(
            args.model_dir / onnx_assets[0]["name"],
            tokenizer,
            contract["pooling"],
            contract["max_length"],
            truncate_dims=contract.get("truncate_dims"),
            pre_truncate_layer_norm=(
                contract.get("pre_truncate_layer_norm") == PRE_TRUNCATE_LAYER_NORM
            ),
        )
        SEMANTIC_PACK = args.semantic_pack
        SEMANTIC_QUERY_PREFIX = contract["query_prefix"]
        SEMANTIC_SPACE_ID = semantic_space_id

    missing = [path for path in (INDEX, CLI, *PACKS) if not path.is_file()]
    if SEMANTIC_PACK is not None:
        missing.extend(path for path in [SEMANTIC_PACK] if not path.is_file())
    if missing:
        raise SystemExit("missing UI dependency: " + ", ".join(map(str, missing)))
    host = "127.0.0.1"
    port = args.port
    print(f"Verse Recall search UI: http://{host}:{port}", flush=True)
    ThreadingHTTPServer((host, port), SearchHandler).serve_forever()


if __name__ == "__main__":
    main()
