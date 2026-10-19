#!/usr/bin/env python3
"""Actual server + loopback Binance wire protocols, gap repair, L2 resync and restart.

No external exchange connection. Standard library only. Optional real Redis AOF.
The fixture deliberately withholds trades 11/12 and depth update 12.
"""
import argparse
import base64
import hashlib
import gzip
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import socket
import sqlite3
import struct
import subprocess
import threading
import time
import zlib
from urllib.parse import parse_qs, urlencode, urlsplit
from accept_pipeline import ROOT, Redis, check, digest, fetch, port, stop as stop_unix, write_json


class Fixture:
    def __init__(self):
        self.start = (int(time.time() * 1000) // 60000 - 2) * 60000
        self.lock = threading.Lock()
        self.break_book = threading.Event()
        self.allow_resync = threading.Event()
        self.stop = threading.Event()
        self.book_connections = self.trade_connections = self.agg_requests = 0
        self.requests = []
        self.rows = [self.trade(n) for n in range(1503)]

    def trade(self, n):
        return {"a": n, "p": "10.0", "q": "1.0", "m": n % 2 == 0,
                "T": self.start - 1 if n == 0 else self.start + 60000 if n == 1502 else self.start + n * 20}


def send_frame(peer, opcode, body):
    length = len(body)
    header = bytes([0x80 | opcode])
    header += bytes([length]) if length < 126 else b"\x7e" + struct.pack("!H", length) if length <= 65535 else b"\x7f" + struct.pack("!Q", length)
    peer.sendall(header + body)


def read_exact(peer, length):
    data = b""
    while len(data) < length:
        part = peer.recv(length - len(data))
        if not part:
            raise ConnectionError("closed websocket")
        data += part
    return data


def read_control(peer):
    header = read_exact(peer, 2)
    opcode, length = header[0] & 15, header[1] & 127
    masked = header[1] & 128
    if length == 126:
        length = struct.unpack("!H", read_exact(peer, 2))[0]
    elif length == 127:
        length = struct.unpack("!Q", read_exact(peer, 8))[0]
    check(length <= 4 * 1024 * 1024, "fixture control frame too large")
    mask = read_exact(peer, 4) if masked else None
    data = read_exact(peer, length)
    if mask:
        data = bytes(c ^ mask[n % 4] for n, c in enumerate(data))
    if opcode == 8:
        raise ConnectionError("peer closed")
    if opcode == 9:
        send_frame(peer, 10, data)


def stop(child):
    if os.name == "nt":
        if child is not None and child.poll() is None:
            # CREATE_NEW_PROCESS_GROUP ignores CTRL_C; the server does not
            # register CTRL_BREAK. Exercise durable crash recovery on Windows.
            child.terminate()
            child.wait(timeout=20)
    else:
        stop_unix(child)


def handler(fixture):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_):
            pass

        def respond(self, status, data, headers=()):
            raw = json.dumps(data, separators=(",", ":")).encode()
            self.send_response(status)
            for key, value in headers:
                self.send_header(key, value)
            self.send_header("Content-Length", str(len(raw)))
            self.send_header("Content-Type", "application/json")
            self.send_header("Connection", "close")
            self.end_headers()
            try:
                self.wfile.write(raw)
            except (BrokenPipeError, ConnectionResetError):
                pass  # Expected when main cancels an outstanding snapshot at shutdown.
            self.close_connection = True

        def do_GET(self):
            path = urlsplit(self.path)
            if path.path in ("/book", "/trades"):
                return self.websocket(path.path == "/book")
            if path.path == "/depth":
                with fixture.lock:
                    connection = fixture.book_connections
                if connection >= 2:
                    fixture.allow_resync.wait(20)
                time.sleep(0.08)  # Force diff buffering before the snapshot.
                self.respond(200, {"lastUpdateId": 10 if connection == 1 else 20,
                                   "bids": [[f"{(9900-n)//100}.{(9900-n)%100:02d}", "1"] for n in range(1000)],
                                   "asks": [[f"{(10100+n)//100}.{(10100+n)%100:02d}", "3"] for n in range(1000)]})
            elif path.path == "/aggTrades":
                query = parse_qs(path.query)
                first = int(query.get("fromId", [0])[0])
                with fixture.lock:
                    fixture.agg_requests += 1
                    request = fixture.agg_requests
                    fixture.requests.append({"fromId": first, "limit": query.get("limit"), "rate_limited": request == 1})
                if request == 1:
                    self.respond(429, {}, [("Retry-After", "1")])
                else:
                    self.respond(200, [r for r in fixture.rows if r["a"] >= first][:1000])
            else:
                self.respond(404, {})

        def websocket(self, book):
            key = self.headers["Sec-WebSocket-Key"]
            accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
            self.send_response(101)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", accept)
            self.end_headers()
            with fixture.lock:
                if book:
                    fixture.book_connections += 1
                    connection = fixture.book_connections
                else:
                    fixture.trade_connections += 1
                    connection = fixture.trade_connections
            peer = self.connection
            peer.settimeout(0.1)

            def send(value):
                send_frame(peer, 1, json.dumps(value, separators=(",", ":")).encode())

            def depth(n):
                send({"e": "depthUpdate", "s": "BTCUSDT", "E": int(time.time() * 1000),
                      "U": n, "u": n, "b": [["99.00", "5"], ["94.00", "7"], ["98.99", "0"]],
                      "a": [["101.00", "6"], ["106.00", "8"], ["101.01", "0"]]})
            try:
                if book:
                    depth(11 if connection == 1 else 21)
                else:
                    for n in ([10, 13] if connection == 1 else [13]):
                        row = fixture.trade(n)
                        send({"stream": "btcusdt@aggTrade", "data": dict(row, s="BTCUSDT", e="aggTrade")})
                        time.sleep(0.05)
                sequence = 21
                next_update = time.monotonic() + 0.25
                broken = False
                while not fixture.stop.is_set():
                    if book and connection == 1 and fixture.break_book.is_set() and not broken:
                        depth(13)
                        broken = True
                    if book and connection >= 2 and time.monotonic() >= next_update:
                        sequence += 1
                        depth(sequence)
                        next_update = time.monotonic() + 0.25
                    try:
                        read_control(peer)
                    except socket.timeout:
                        continue
            except (OSError, ConnectionError):
                pass
            self.close_connection = True
    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug" / ("market-bridge.exe" if os.name == "nt" else "market-bridge"))
    parser.add_argument("--redis-server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    fixture = Fixture()
    peer = ThreadingHTTPServer(("127.0.0.1", 0), handler(fixture))
    thread = threading.Thread(target=peer.serve_forever, daemon=True)
    thread.start()
    peer_base = f"127.0.0.1:{peer.server_port}"
    api_port = port()
    base = f"http://127.0.0.1:{api_port}"
    config = (ROOT / "config.research.yaml").read_text(encoding="utf-8").replace("api_rate_limit_per_minute: 120", "api_rate_limit_per_minute: 0")
    env = {k: v for k, v in os.environ.items() if not k.startswith("MARKETBRIDGE_")}
    children, logs = [], []
    server = redis = None
    db = directory / ("outbox.sqlite" if args.redis_server else "recovery.sqlite")

    def launch(command, label):
        log = (directory / f"{label}-{len(children)}.log").open("w", encoding="utf-8")
        logs.append(log)
        flags = {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == "nt" else {}
        child = subprocess.Popen([str(c) for c in command], cwd=ROOT, env=env, stdout=log, stderr=log, **flags)
        children.append(child)
        return child

    def wait_for(description, function, timeout=35):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            check(server is None or server.poll() is None, "main server exited: inspect process logs")
            try:
                result = function()
                if result:
                    return result
            except OSError:
                pass
            time.sleep(0.04)
        raise RuntimeError("timeout: " + description)

    def get(path):
        return fetch(base, path)[:2]

    try:
        if args.redis_server:
            redis_port = port()
            redis_dir = directory / "redis"
            redis_dir.mkdir()
            redis = launch([args.redis_server.resolve(), "--bind", "127.0.0.1", "--port", redis_port,
                            "--dir", redis_dir, "--save", "", "--appendonly", "yes", "--appendfsync", "everysec"], "redis")
            def redis_ready():
                conn = Redis(("127.0.0.1", redis_port))
                try:
                    return conn.command("PING") == b"PONG"
                finally:
                    conn.close()
            wait_for("Redis startup", redis_ready)
            config = config.replace("runtime:\n", f'runtime:\n  redis_url: "redis://127.0.0.1:{redis_port}/"\n  redis_stream_prefix: native_acceptance\n', 1)
        config = config.replace("runtime:\n", "runtime:\n  order_books:\n    publish_levels: 1000\n", 1)
        config_path = directory / "config.yaml"
        config_path.write_text(config, encoding="utf-8")
        fixture_path = directory / "fixture.json"
        write_json(fixture_path, {"symbol": "BTCUSDT", "book_ws": f"ws://{peer_base}/book", "trade_ws": f"ws://{peer_base}/trades",
                                  "depth_rest": f"http://{peer_base}/depth", "agg_rest": f"http://{peer_base}/aggTrades"})
        env.update(MARKETBRIDGE_CONFIG=str(config_path), MARKETBRIDGE_API_ADDR=f"127.0.0.1:{api_port}",
                   MARKETBRIDGE_RESEARCH_DB=str(directory / "research.sqlite"), MARKETBRIDGE_OUTBOX_DB=str(db),
                   MARKETBRIDGE_RECOVERY_DB=str(db))
        write_json(directory / "metadata.json", {"scope": "actual main process with loopback Binance spot WS/REST protocol fixtures; no public exchange claim",
                   "binary_sha256": digest(binary), "harness_sha256": digest(Path(__file__)), "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                   "source_hashes": {str(p.relative_to(ROOT)): digest(p) for p in sorted((ROOT / "src").rglob("*.rs"))},
                   "redis_aof": bool(args.redis_server), "process_stop": "TerminateProcess crash recovery" if os.name == "nt" else "SIGTERM graceful restart", "started_at_unix": time.time()})
        server = launch([binary, "--protocol-fixture", fixture_path], "main")
        wait_for("HTTP startup", lambda: get("/health")[0] == 200)
        initial = wait_for("initial L2 snapshot + buffered delta", lambda: (lambda value: value if value["books"] and value["books"][0]["book_quality"]["synchronized"] else None)(get("/v1/market/order-books")[1]))
        book = initial["books"][0]
        check(book["bids"][0] == {"price": 99.0, "qty": 5.0} and book["asks"][0] == {"price": 101.0, "qty": 6.0}, "buffered delta was not merged into REST snapshot")
        check(len(book["bids"]) == len(book["asks"]) == 999, "full 1000-level reconstruction or zero-size deletion failed")
        check({row["price"]: row["qty"] for row in book["bids"]}[94.0] == 7 and {row["price"]: row["qty"] for row in book["asks"]}[106.0] == 8, "deep delta was not merged into the complete book")
        check(all(row["price"] != 98.99 for row in book["bids"]) and all(row["price"] != 101.01 for row in book["asks"]), "zero-size depth did not delete the exact decimal level")
        first_session = book["book_quality"]["session"]
        write_json(directory / "initial-book.json", initial)
        fixture.break_book.set()
        invalid = wait_for("old book immediately invalidated after native depth gap", lambda: (lambda value: value if value["books"] and not value["books"][0]["book_quality"]["synchronized"] else None)(get("/v1/market/order-books")[1]))
        write_json(directory / "invalidated-book.json", invalid)
        check(get("/ready")[0] == 503, "gap must prevent live readiness")
        fixture.allow_resync.set()
        rebuilt = wait_for("L2 rebootstrap using a new connection", lambda: (lambda value: value if value["books"] and value["books"][0]["book_quality"]["synchronized"] and value["books"][0]["book_quality"]["session"] != first_session else None)(get("/v1/market/order-books")[1]))
        write_json(directory / "rebuilt-book.json", rebuilt)
        scope = {"exchange": "binance", "market": "spot", "symbol": "BTCUSDT", "start_ms": fixture.start, "end_ms": fixture.start + 60000}
        path = "/v1/market/order-flow/recovered-window?" + urlencode(scope)
        proof = wait_for("automatic REST repair after 429, paginated closure and verification", lambda: (lambda result: result[1] if result[0] == 200 else None)(get(path)))
        totals = proof["window"]["recovered"]
        check(totals["trade_count"] == 1501 and totals["buy_qty"] == 751 and totals["sell_qty"] == 750, "REST reconstructed totals differ from native ledger")
        check(proof["source_quality"]["has_known_loss"] and not proof["global_quality_cleared"] and not proof["window"]["quality"]["lifetime_cvd_repaired"], "finite recovery incorrectly cleared global loss")
        check(proof["window"]["quality"]["before_sequence"] == 0 and proof["window"]["quality"]["after_sequence"] == 1502, "native window boundary proof missing")
        write_json(directory / "recovered-window.json", proof)
        status = get("/v1/system/recovery")[1]
        write_json(directory / "recovery-status.json", status)
        job = status["jobs"][0]
        check(job["verified"] and job["attempt"] >= 2 and fixture.agg_requests >= 4, "automatic retry/pagination not exercised")
        # Online status is read-only and does not take the delivery lease.
        result = subprocess.run([str(binary), "--recovery", "status", str(db)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=10)
        check(result.returncode == 0 and json.loads(result.stdout)["jobs"][0]["verified"], "read-only online CLI status failed")
        locked = subprocess.run([str(binary), "--recovery", "retry", str(db), str(job["id"])], cwd=ROOT, env=env, capture_output=True, text=True, timeout=10)
        check(locked.returncode != 0, "offline retry must not acquire live database lease")
        stop(server)
        server = None
        with sqlite3.connect(db) as conn:
            check(conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "SQLite evidence corrupt")
            check(conn.execute("SELECT COUNT(*) FROM recovery_records").fetchone()[0] == 1503, "incomplete persisted raw proof")
            checkpoint = json.loads(conn.execute("SELECT checkpoint FROM source_checkpoints").fetchone()[0])
            check(checkpoint["sequence"] == 13, "native checkpoint was not committed")
            origin = conn.execute("SELECT origin FROM outbox_state WHERE id=1").fetchone()[0]
            evidence = []
            for job_id, sequence, body, checksum in conn.execute("SELECT job_id,sequence,body,crc32 FROM recovery_records ORDER BY job_id,sequence"):
                check(zlib.crc32(struct.pack("<QQ", job_id, sequence) + body.encode()) == checksum, "raw proof CRC failed before export")
                evidence.append(json.dumps({"job_id": job_id, "sequence": sequence, "body": body, "crc32": checksum}, separators=(",", ":")))
            (directory / "verified-native-records.jsonl.gz").write_bytes(gzip.compress(("\n".join(evidence) + "\n").encode(), mtime=0))
        retry = subprocess.run([str(binary), "--recovery", "retry", str(db), str(job["id"])], cwd=ROOT, env=env, capture_output=True, text=True, timeout=10)
        check(retry.returncode != 0, "verified evidence must not be overwritten by retry")
        requests_before_restart = fixture.agg_requests
        server = launch([binary, "--protocol-fixture", fixture_path], "restart")
        wait_for("restart HTTP", lambda: get("/health")[0] == 200)
        restarted = wait_for("persisted verified proof after restart", lambda: (lambda result: result[1] if result[0] == 200 else None)(get(path)))
        check(restarted["window"] == proof["window"], "verified totals/CRC changed across restart")
        wait_for("restart WS duplicate received", lambda: fixture.trade_connections >= 2)
        time.sleep(0.3)
        check(fixture.agg_requests == requests_before_restart, "verified job was needlessly downloaded again")
        check(len(get("/v1/system/recovery")[1]["jobs"]) == 1, "duplicate trade created recovery job")
        write_json(directory / "restart-window.json", restarted)
        if args.redis_server:
            def drained():
                status = get("/v1/system/outbox")[1]
                return status if status.get("status", {}).get("pending_records") == 0 else None
            wait_for("durable outbox drain", drained)
            conn = Redis(("127.0.0.1", redis_port))
            native_ids, audit_events, book_events = [], 0, 0
            durable_evidence, event_ids = [], set()
            for key in conn.command("KEYS", "native_acceptance*:origin:*"):
                if conn.command("TYPE", key) != b"stream":
                    continue
                for native_id, fields in conn.command("XRANGE", key, "-", "+"):
                    row = dict(zip(fields[::2], fields[1::2]))
                    event_id = row[b"event_id"].decode()
                    check(event_id not in event_ids, "duplicate stable event identity in Redis")
                    event_ids.add(event_id)
                    sequence = int(native_id.split(b"-")[0])
                    check(event_id == f"{origin}:{sequence}", "durable identity changed")
                    payload = row[b"payload"].decode()
                    logical = key.decode().split(":origin:")[0]
                    checksum_input = [{"origin": origin, "sequence": sequence}, logical, int(row[b"received_at_ms"]), payload]
                    check(zlib.crc32(json.dumps(checksum_input, ensure_ascii=False, separators=(",", ":")).encode()) == int(row[b"event_crc32"]), "Redis event evidence CRC mismatch")
                    durable_evidence.append(json.dumps({"stream": key.decode(), "stream_id": native_id.decode(), "fields": {k.decode(): v.decode() for k, v in row.items()}}, ensure_ascii=False, separators=(",", ":")))
                    event = json.loads(payload)
                    if event["type"] == "trade":
                        native_ids.append(int(event["trade_id"]))
                    elif event["type"] == "order_book":
                        book_events += 1
                    elif event.get("category") == "order_book_integrity":
                        audit_events += 1
            conn.close()
            (directory / "redis-events.jsonl.gz").write_bytes(gzip.compress(("\n".join(durable_evidence) + "\n").encode(), mtime=0))
            check(sorted(native_ids) == [10, 13], "REST history or duplicate restart trade leaked into live durable stream")
            check(book_events >= 2 and audit_events >= 2, "book reconstruction evidence missing from actual Redis")
        else:
            book_events = audit_events = None
        stop(server)
        server = None
        # Prove the HTTP endpoint rechecks raw evidence, not only the old seal.
        with sqlite3.connect(db) as conn:
            conn.execute("UPDATE recovery_records SET crc32=crc32+1 WHERE sequence=11")
        server = launch([binary, "--protocol-fixture", fixture_path], "corrupt-proof")
        wait_for("corrupt-proof HTTP startup", lambda: get("/health")[0] == 200)
        code, corrupt = get(path)
        check(code == 503 and corrupt.get("recovered") is None, "corrupt recovery evidence exposed numeric results")
        write_json(directory / "corrupt-proof-response.json", {"http_status": code, "body": corrupt})
        write_json(directory / "provider-requests.json", fixture.requests)
        report = {"passed": True, "scope": "localhost actual main process, production Binance spot protocol codecs",
                  "global_quality_cleared": False, "recovered_trades": 1501, "native_boundary_ids": [0, 1502],
                  "rest_requests": fixture.agg_requests, "rate_limit_retry_exercised": True,
                  "snapshot_levels_per_side": 1000, "published_levels_after_zero_delete_per_side": 999,
                  "deep_level_updates_verified": True, "order_book_gap_invalidated_old_session": True, "order_book_resynchronized_new_session": True,
                  "restart_retained_native_checkpoint_and_proof": True, "corrupt_evidence_http_status": code,
                  "sqlite_integrity": "ok before deliberate corruption", "store_origin": origin,
                  "real_redis_aof": bool(args.redis_server), "redis_book_events": book_events, "redis_book_audit_events": audit_events}
        write_json(directory / "report.json", report)
        print(json.dumps(report, ensure_ascii=False))
    except BaseException as error:
        write_json(directory / "failure.json", {"error": str(error)})
        raise
    finally:
        fixture.stop.set()
        fixture.allow_resync.set()
        for child in reversed(children):
            stop(child)
        peer.shutdown()
        peer.server_close()
        for log in logs:
            log.close()


if __name__ == "__main__":
    main()
