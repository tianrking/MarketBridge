#!/usr/bin/env python3
"""Run actual MarketBridge + SQLite FULL/WAL + Redis AOF + HTTP acceptance.

Standard library only; launches/stops its own children on loopback. Synthetic
fixture data never represents an exchange connection. See docs/pipeline_acceptance.zh-CN.md.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import socket
import sqlite3
import subprocess
import time
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import ProxyHandler, Request, build_opener
import zlib

ROOT = Path(__file__).resolve().parents[1]
HTTP = build_opener(ProxyHandler({}))


def check(condition, message):
    if not condition:
        raise RuntimeError(message)


def port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


class Redis:
    def __init__(self, address):
        self.socket = socket.create_connection(address, timeout=10)
        self.file = self.socket.makefile("rb")

    def read(self):
        kind = self.file.read(1)
        check(kind, "Redis closed connection")
        line = self.file.readline().rstrip(b"\r\n")
        if kind == b"+":
            return line
        if kind == b"-":
            raise RuntimeError("Redis error: " + line.decode())
        if kind == b":":
            return int(line)
        n = int(line)
        if n == -1:
            return None
        if kind == b"$":
            value = self.file.read(n)
            check(self.file.read(2) == b"\r\n", "Redis frame corrupt")
            return value
        check(kind == b"*", "invalid Redis response")
        return [self.read() for _ in range(n)]

    def command(self, *args):
        parts = [str(a).encode() if not isinstance(a, bytes) else a for a in args]
        self.socket.sendall(b"*%d\r\n" % len(parts) + b"".join(b"$%d\r\n" % len(p) + p + b"\r\n" for p in parts))
        return self.read()

    def close(self):
        self.file.close()
        self.socket.close()


def fetch(base, path, text=False):
    started = time.monotonic()
    try:
        response = HTTP.open(Request(base + path), timeout=10)
    except HTTPError as error:
        response = error
    with response:
        raw = response.read().decode()
        return response.code, raw if text else json.loads(raw), (time.monotonic() - started) * 1000


def proc_stats(child):
    if platform.system() != "Linux":
        return {"available": False}
    try:
        fields = Path(f"/proc/{child.pid}/stat").read_text().split()
        status = Path(f"/proc/{child.pid}/status").read_text()
        hz = os.sysconf("SC_CLK_TCK")
        return {"available": True, "cpu_seconds": (int(fields[13]) + int(fields[14])) / hz,
                "rss_bytes": int(re.search(r"VmRSS:\s+(\d+)", status)[1]) * 1024}
    except (OSError, TypeError):
        return {"available": False}


def size(directory):
    return sum(p.stat().st_size for p in directory.rglob("*") if p.is_file())


def stop(child):
    if child is None or child.poll() is not None:
        return
    if os.name == "nt":
        child.send_signal(signal.CTRL_BREAK_EVENT)
    else:
        child.terminate()
    try:
        child.wait(timeout=20)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait(timeout=10)
        raise RuntimeError("child failed graceful shutdown")
    check(child.returncode == 0, f"child exited {child.returncode}")


def metric_sum(metrics, name, labels=None):
    total = 0.0
    for line in metrics.splitlines():
        if line.startswith(name + " ") or line.startswith(name + "{"):
            if labels is None or all(f'{k}="{v}"' in line for k, v in labels.items()):
                total += float(line.rsplit(" ", 1)[1])
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/market-bridge")
    parser.add_argument("--redis-server", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--duration", type=int, default=600)
    parser.add_argument("--rate", type=int, default=1000)
    parser.add_argument("--symbols", type=int, default=16)
    parser.add_argument("--sample-seconds", type=float, default=5)
    parser.add_argument("--outage-seconds", type=float, default=5)
    parser.add_argument("--max-disk-mb", type=int, default=8192)
    parser.add_argument("--redis-maxmemory-mb", type=int, default=2048)
    args = parser.parse_args()
    check(args.duration >= 15 and args.rate > 0 and 1 <= args.symbols <= 2048, "invalid acceptance load")
    check(args.sample_seconds > 0 and 0 < args.outage_seconds < args.duration / 3, "invalid sample/outage interval")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    args.binary = args.binary.resolve()
    args.redis_server = args.redis_server.resolve()
    rport, aport = port(), port()
    check(rport != aport, "ephemeral port collision; rerun")
    base = f"http://127.0.0.1:{aport}"
    redis_dir = args.output / "redis"
    redis_dir.mkdir()
    log_files = []
    children = []
    server = redis = None
    env = dict(os.environ)
    # Hermetic local config; do not inherit production endpoints or record dirs.
    for key in list(env):
        if key.startswith("MARKETBRIDGE_"):
            env.pop(key)
    config = (ROOT / "config.research.yaml").read_text(encoding="utf-8")
    config = config.replace("runtime:\n", f'runtime:\n  redis_url: "redis://127.0.0.1:{rport}/"\n  redis_stream_prefix: acceptance\n  outbox:\n    max_pending_bytes: 268435456\n', 1)
    config = config.replace("api_rate_limit_per_minute: 120", "api_rate_limit_per_minute: 0")
    config_path = args.output / "config.yaml"
    config_path.write_text(config, encoding="utf-8")
    env.update(MARKETBRIDGE_CONFIG=str(config_path), MARKETBRIDGE_API_ADDR=f"127.0.0.1:{aport}",
               MARKETBRIDGE_RESEARCH_DB=str(args.output / "research.sqlite"),
               MARKETBRIDGE_OUTBOX_DB=str(args.output / "outbox.sqlite"))
    fixture_path = args.output / "fixture.json"
    total = args.duration * args.rate
    write_json(fixture_path, {"events": total, "events_per_second": args.rate, "symbols": args.symbols})
    meta = {"scope": "actual server pipeline with explicit synthetic JSON codec; no exchange networking",
            "arguments": {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items()},
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "harness_sha256": digest(Path(__file__)),
            "platform": platform.platform(), "python": platform.python_version(), "cpu_count": os.cpu_count(),
            "binary_sha256": digest(args.binary), "redis_sha256": digest(args.redis_server),
            "source_hashes": {str(p.relative_to(ROOT)): digest(p) for p in sorted((ROOT / "src").rglob("*.rs"))},
            "redis_appendfsync": "everysec with WAITAOF required for local ACK", "started_at_unix": time.time()}
    for name in ("cpu.max", "memory.max"):
        p = Path("/sys/fs/cgroup") / name
        meta[name] = p.read_text().strip() if p.exists() else None
    write_json(args.output / "metadata.json", meta)

    def launch(command, label):
        log = (args.output / f"{label}-{len(children)}.log").open("w", encoding="utf-8")
        log_files.append(log)
        flags = {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == "nt" else {}
        child = subprocess.Popen([str(c) for c in command], cwd=ROOT, env=env, stdout=log, stderr=log, **flags)
        children.append(child)
        return child

    def start_redis():
        child = launch([args.redis_server, "--bind", "127.0.0.1", "--port", rport, "--dir", redis_dir,
                        "--save", "", "--appendonly", "yes", "--appendfsync", "everysec",
                        "--maxmemory", f"{args.redis_maxmemory_mb}mb", "--maxmemory-policy", "noeviction"], "redis")
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            check(child.poll() is None, "Redis exited at startup")
            try:
                conn = Redis(("127.0.0.1", rport))
                try:
                    conn.command("PING")
                    return child
                except RuntimeError as error:
                    if not str(error).startswith("Redis error: LOADING "):
                        raise
                finally:
                    conn.close()
            except OSError:
                pass
            time.sleep(.05)
        raise RuntimeError("Redis startup timed out")

    def start_server():
        child = launch([args.binary, "--acceptance-fixture", fixture_path], "server")
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            check(child.poll() is None, "server exited at startup; inspect log")
            try:
                if fetch(base, "/health")[0] == 200:
                    return child
            except OSError:
                time.sleep(.05)
        raise RuntimeError("HTTP startup timed out")

    samples = []
    faults = []
    try:
        redis = start_redis()
        server = start_server()
        started = time.monotonic()
        fault_at = min(30, args.duration / 3)
        redis_restart_at = None
        last_sample = -args.sample_seconds
        peak_pending = 0
        saw_unready = False
        stable_ready_samples = 0
        while True:
            elapsed = time.monotonic() - started
            check(server.poll() is None, "server failed during acceptance")
            if elapsed >= fault_at and not faults:
                stop(redis)
                redis_restart_at = time.monotonic() + args.outage_seconds
                faults.append({"kind": "Redis graceful outage", "elapsed_seconds": elapsed})
            if redis_restart_at is not None:
                outage_code, _, _ = fetch(base, "/ready")
                check(fetch(base, "/health")[0] == 200,"liveness failed during Redis outage")
                saw_unready = saw_unready or outage_code == 503
            if redis_restart_at is not None and time.monotonic() >= redis_restart_at:
                redis = start_redis()
                redis_restart_at = None
                faults.append({"kind": "Redis AOF restart", "elapsed_seconds": elapsed})
            if elapsed - last_sample >= args.sample_seconds or elapsed >= args.duration:
                last_sample = elapsed
                code, ready, http_ms = fetch(base, "/ready")
                _, outbox, _ = fetch(base, "/v1/system/outbox")
                _, memory, _ = fetch(base, "/v1/system/memory")
                _, metrics, _ = fetch(base, "/metrics", text=True)
                _, states, query_ms = fetch(base, "/v1/research/symbol-state")
                pending = outbox["status"]["pending_records"]
                peak_pending = max(peak_pending, pending)
                if len(faults) == 1 and code == 503:
                    saw_unready = True
                if elapsed > 3 and not faults and code == 200:
                    stable_ready_samples += 1
                check(not ready["ingestion"]["integrity"]["has_known_loss"], "source integrity loss")
                check(not memory["strategy"]["quality"]["has_capacity_loss"], "strategy capacity loss")
                check(not states["subscription_quality"]["has_known_gap"], "strategy subscriber loss")
                check(not ready["history_quality"]["order_flow"]["has_capacity_loss"], "bucket retention loss")
                check(metric_sum(metrics, "ticks_dropped_total") == 0, "admission drops")
                check(memory["aggregator"]["accounted_bytes"] <= memory["order_flow"]["limits"]["aggregator_bytes"], "aggregator budget exceeded")
                check(memory["aggregator"]["capacity_evictions"] == 0, "aggregator capacity loss")
                sample = {"elapsed_seconds": elapsed, "ready_http_status": code, "ready": ready,
                          "memory": memory, "metrics": metrics, "http_ready_ms": http_ms, "http_states_ms": query_ms,
                          "server": proc_stats(server), "redis": proc_stats(redis), "disk_bytes": size(args.output)}
                check(sample["disk_bytes"] <= args.max_disk_mb * 1024 * 1024, "acceptance disk guard reached")
                with (args.output / "samples.jsonl").open("a", encoding="utf-8") as output:
                    output.write(json.dumps(sample, ensure_ascii=False) + "\n")
                samples.append({k: v for k, v in sample.items() if k not in ("metrics", "ready", "memory")})
                print(json.dumps({"elapsed": round(elapsed, 1), "committed": outbox["status"]["last_assigned_sequence"],
                                  "pending": pending, "ready": code, "rss": sample["server"].get("rss_bytes")}), flush=True)
                stages_drained = all(metric_sum(metrics,"pipeline_latency_seconds_count",{"stage":stage}) == total for stage in ("router","bus","aggregator","strategy_state"))
                if outbox["status"]["last_assigned_sequence"] == total and pending == 0 and stages_drained:
                    break
            check(elapsed < args.duration + max(60, args.duration * .2), "pipeline failed to meet drain deadline")
            time.sleep(min(.25, args.sample_seconds))
        drain_elapsed = time.monotonic() - started
        check(saw_unready, "Redis outage was not reflected by /ready; sample more frequently")
        check(stable_ready_samples > 0, "no healthy steady sample before fault")
        _, final_ready, _ = fetch(base, "/ready")
        check(final_ready["data_ready"], "data not ready after drain")
        _, metrics, _ = fetch(base, "/metrics", text=True)
        check(metric_sum(metrics, "outbox_committed_total") == total, "outbox committed count mismatch")
        check(metric_sum(metrics, "redis_xadd_total") == total, "durable ACK count mismatch")
        for stage in ("router", "bus", "aggregator", "strategy_state"):
            check(metric_sum(metrics, "pipeline_latency_seconds_count", {"stage": stage}) == total, f"{stage} count mismatch")
        origin = final_ready["storage"]["status"]["origin"]
        # Restart the real server against the same SQLite and Redis identity.
        stop(server)
        more = args.symbols * 20
        write_json(fixture_path, {"events": more, "events_per_second": args.rate, "symbols": args.symbols})
        server = start_server()
        deadline = time.monotonic() + 40
        while True:
            _, state, _ = fetch(base, "/v1/system/outbox")
            if state["status"]["last_assigned_sequence"] == total + more and state["status"]["pending_records"] == 0:
                break
            check(time.monotonic() < deadline, "post-restart delivery did not drain")
            time.sleep(.25)
        _, after, _ = fetch(base, "/ready")
        check(after["data_ready"], "post-restart native continuity/readiness failed")
        check(after["storage"]["status"]["origin"] == origin, "durable origin changed on restart")
        faults.append({"kind": "server graceful restart with persisted native checkpoints", "additional_events": more})
        # Recover a finite interval from the new process, independently of the
        # older process's retained histories.
        conn = Redis(("127.0.0.1", rport))
        keys = []
        cursor = 0
        while True:
            cursor, batch = conn.command("SCAN", cursor, "MATCH", "acceptance:*:origin:*", "COUNT", 100)
            keys.extend(batch)
            if int(cursor) == 0:
                break
        expected = total + more
        seen = bytearray(expected + 1)
        domains = {}
        native = {}
        recovered_rows = []
        first_ts = last_ts = None
        for key in sorted(set(keys)):
            previous = "-"
            while True:
                rows = conn.command("XRANGE", key, previous, "+", "COUNT", 512)
                if not rows:
                    break
                for rid, fields in rows:
                    row = dict(zip(fields[::2], fields[1::2]))
                    sequence = int(rid.split(b"-")[0])
                    check(1 <= sequence <= expected and seen[sequence] == 0, "duplicate or invalid durable identity")
                    seen[sequence] = 1
                    check(row[b"event_id"].decode() == f"{origin}:{sequence}", "Redis event identity mismatch")
                    payload = row[b"payload"].decode()
                    logical = key.decode().split(":origin:")[0]
                    evidence = [{"origin": origin, "sequence": sequence}, logical, int(row[b"received_at_ms"]), payload]
                    checksum = zlib.crc32(json.dumps(evidence, ensure_ascii=False, separators=(",", ":")).encode())
                    check(checksum == int(row[b"event_crc32"]), "durable payload CRC mismatch")
                    event = json.loads(payload)
                    kind = event["type"]
                    domains[kind] = domains.get(kind, 0) + 1
                    if kind == "trade":
                        stream = event["symbol"]
                        nid = int(event["trade_id"])
                        check(nid == native.get(stream, 0) + 1, "native trade gap/duplicate across restart")
                        native[stream] = nid
                        if sequence > total and stream == "FIX0000USDT":
                            recovered_rows.append(event)
                    first_ts = min(first_ts or event["ts_ms"], event["ts_ms"])
                    last_ts = max(last_ts or event["ts_ms"], event["ts_ms"])
                previous = "(" + rows[-1][0].decode()
        conn.close()
        check(sum(seen) == expected, "missing durable events across streams/restart")
        check(len(recovered_rows) >= 3, "insufficient recovery probe trades")
        start_ms, end_ms = recovered_rows[1]["ts_ms"], recovered_rows[-1]["ts_ms"]
        # Millisecond batching can share timestamps. Use independently bounded
        # times only when start/end leave actual native boundary trades.
        start_ms = recovered_rows[0]["ts_ms"] + 1
        check(start_ms < end_ms, "recovery probe duration too short; lower fixture rate")
        query = urlencode(dict(exchange="acceptance_fixture", market="perp", symbol="FIX0000USDT", start_ms=start_ms, end_ms=end_ms))
        code, proof, _ = fetch(base, "/v1/market/order-flow/verified-window?" + query)
        check(code == 200 and proof["window"]["quality"]["verified"], "post-restart bounded recovery failed")
        rows = [r for r in recovered_rows if start_ms <= r["ts_ms"] < end_ms]
        totals = proof["window"]["recovered"]
        check(totals["trade_count"] == len(rows), "recovered trade count differs from durable ledger")
        check(totals["buy_qty"] == sum(r["qty"] for r in rows if r["side"] == "buy"), "recovered buy qty differs")
        check(totals["sell_qty"] == sum(r["qty"] for r in rows if r["side"] == "sell"), "recovered sell qty differs")
        check(metric_sum(metrics, "pipeline_latency_seconds_count", {"stage": "order_flow"}) == domains["trade"] - sum(1 for n in range(more) if (n // args.symbols) % 10 < 6), "order-flow processed count mismatch")
        write_json(args.output / "final-ready.json", final_ready)
        (args.output / "final-metrics.txt").write_text(metrics, encoding="utf-8")
        write_json(args.output / "window-proof.json", proof)
        stop(server)
        stop(redis)
        for name in ("outbox.sqlite", "research.sqlite"):
            with sqlite3.connect(args.output / name) as db:
                check(db.execute("PRAGMA integrity_check").fetchone()[0] == "ok", f"{name} integrity failed")
        rss = [s["server"]["rss_bytes"] for s in samples if s["server"].get("available")]
        http = sorted(s["http_states_ms"] for s in samples)
        report = {"passed": True, "scope": meta["scope"], "requested_duration_seconds": args.duration,
                  "elapsed_ingestion_and_drain_seconds": drain_elapsed,
                  "events_before_restart": total, "events_verified_after_restart": expected, "domains": domains,
                  "global_identity_missing": 0, "global_identity_duplicates": 0, "native_trade_gaps": 0,
                  "peak_pending_records": peak_pending, "peak_server_rss_bytes": max(rss) if rss else None,
                  "state_http_p95_ms": http[min(len(http) - 1, int(len(http) * .95))],
                  "faults": faults, "steady_ready_samples_before_fault": stable_ready_samples,
                  "first_source_ts_ms": first_ts, "last_source_ts_ms": last_ts,
                  "binary_sha256": meta["binary_sha256"], "disk_bytes": size(args.output),
                  "limits": ["synthetic fixture codec; no exchange network", "Linux resource sampling when available", "finite run; no production SLA", "graceful process restarts; ACK-loss replay is separately tested against real Redis"]}
        write_json(args.output / "report.json", report)
        print(json.dumps(report, ensure_ascii=False), flush=True)
    except BaseException as error:
        write_json(args.output / "failure.json", {"passed": False, "error": str(error), "faults": faults})
        raise
    finally:
        for child in reversed(children):
            if child.poll() is None:
                try:
                    stop(child)
                except (OSError, RuntimeError):
                    child.kill()
                    child.wait(timeout=10)
        for f in log_files:
            f.close()


if __name__ == "__main__":
    main()
