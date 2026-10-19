"""Portable process checks; Unix also verifies SIGTERM recording recovery.

Run after cargo build --locked. Only child processes started here are stopped.
"""

import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "debug" / ("market-bridge.exe" if os.name == "nt" else "market-bridge")


class ProcessLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="marketbridge-process-")
        self.state = Path(self.directory.name)
        self.children = []
        self.logs = []
        self.environment = dict(os.environ)
        for name in ("MARKETBRIDGE_RECORD_DIR", "MARKETBRIDGE_CONTROL_FILE"):
            self.environment.pop(name, None)
        self.environment.update(
            MARKETBRIDGE_CONFIG=str(ROOT / "config.research.yaml"),
            MARKETBRIDGE_RESEARCH_DB=str(self.state / "workspace.sqlite"),
            MARKETBRIDGE_OUTBOX_DB=str(self.state / "outbox.sqlite"),
        )

    def tearDown(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=10)
        for log in self.logs:
            log.close()
        self.directory.cleanup()

    def launch(self):
        log = (self.state / f"process-{len(self.children)}.log").open("w")
        self.logs.append(log)
        child = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, env=self.environment, stdout=log, stderr=log
        )
        self.children.append(child)
        return child

    def ready(self, child, port):
        headers = {}
        if self.environment.get("MARKETBRIDGE_API_KEY"):
            headers["x-api-key"] = self.environment["MARKETBRIDGE_API_KEY"]
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            self.assertIsNone(child.poll(), "server exited before becoming ready")
            try:
                request = Request(f"http://127.0.0.1:{port}/v1/system/info", headers=headers)
                with urlopen(request, timeout=1) as response:
                    self.assertEqual(json.load(response)["service"], "MarketBridge")
                return
            except OSError:
                time.sleep(0.05)
        self.fail("server did not become ready within 15 seconds")

    def test_occupied_api_port_fails_before_recording_starts(self):
        with socket.socket() as occupied:
            occupied.bind(("127.0.0.1", 0))
            occupied.listen()
            self.environment.update(
                MARKETBRIDGE_API_ADDR=f"127.0.0.1:{occupied.getsockname()[1]}",
                MARKETBRIDGE_RECORD_DIR=str(self.state / "recordings"),
            )
            child = self.launch()
            self.assertNotEqual(child.wait(timeout=15), 0, "an unavailable API must fail startup")
        self.assertFalse((self.state / "recordings").exists())

    def test_load_test_cli_reports_delivery_and_rejects_invalid_options(self):
        result = subprocess.run(
            [str(BINARY), "load-test", "--events", "16", "--subscribers", "2",
             "--broadcast-capacity", "32", "--profile", "mixed", "--symbols", "8"],
            cwd=ROOT, env=self.environment, check=True, capture_output=True, text=True, timeout=15,
        )
        report = json.loads(result.stdout)
        self.assertEqual(report["profile"], "mixed")
        self.assertEqual(report["subscriber_deliveries_observed"], 32)
        self.assertTrue(report["lossless"])
        self.assertTrue(report["passed"])
        invalid = subprocess.run(
            [str(BINARY), "load-test", "--events", "invalid"],
            cwd=ROOT, env=self.environment, capture_output=True, text=True, timeout=15,
        )
        self.assertNotEqual(invalid.returncode, 0)

    def test_enabled_sink_initialization_failure_is_supervised(self):
        config = (ROOT / "config.research.yaml").read_text(encoding="utf-8")
        config = config.replace("runtime:\n", 'runtime:\n  redis_url: "invalid://fixture"\n', 1)
        config_path = self.state / "invalid-sink.yaml"
        config_path.write_text(config, encoding="utf-8")
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        self.environment.update(
            MARKETBRIDGE_CONFIG=str(config_path),
            MARKETBRIDGE_API_ADDR=f"127.0.0.1:{port}",
        )
        child = self.launch()
        self.assertNotEqual(child.wait(timeout=15), 0)
        log = (self.state / "process-0.log").read_text(encoding="utf-8")
        self.assertIn("redis_sink", log)

    def test_empty_market_responses_expose_subscription_quality(self):
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        self.environment["MARKETBRIDGE_API_ADDR"] = f"127.0.0.1:{port}"
        child = self.launch()
        self.ready(child, port)
        headers = {}
        if self.environment.get("MARKETBRIDGE_API_KEY"):
            headers["x-api-key"] = self.environment["MARKETBRIDGE_API_KEY"]
        for path, rows in (
            ("/v1/market/order-flow", "order_flow"),
            ("/v1/market/order-flow/windows", "order_flow"),
            ("/v1/market/footprint", "footprints"),
            ("/v1/research/symbol-state", "states"),
        ):
            with self.subTest(path=path):
                request = Request(f"http://127.0.0.1:{port}{path}", headers=headers)
                with urlopen(request, timeout=5) as response:
                    payload = json.load(response)
                self.assertEqual(payload[rows], [])
                quality = payload["subscription_quality"]
                self.assertFalse(quality["has_known_gap"])
                self.assertEqual(quality["gap_epoch"], 0)
                self.assertEqual(quality["upstream_completeness"], "not_verified")
        request = Request(f"http://127.0.0.1:{port}/v1/system/memory", headers=headers)
        with urlopen(request, timeout=5) as response:
            memory = json.load(response)
        self.assertEqual(memory["order_flow"]["raw_records"], 0)
        self.assertEqual(memory["strategy"]["accounted_bytes"], 0)
        self.assertGreater(memory["order_flow"]["limits"]["order_flow_raw_bytes"], 0)
        request = Request(f"http://127.0.0.1:{port}/v1/research/squeeze/scan", headers=headers)
        with urlopen(request, timeout=5) as response:
            self.assertEqual(json.load(response)["model_version"], "squeeze-radar/v2")

    def test_outbox_api_lock_and_checked_offline_cli_roundtrip(self):
        config = (ROOT / "config.research.yaml").read_text(encoding="utf-8")
        config = config.replace("runtime:\n", "runtime:\n  outbox:\n    enabled: true\n", 1)
        config_path = self.state / "outbox.yaml"
        config_path.write_text(config, encoding="utf-8")
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        self.environment.update(MARKETBRIDGE_CONFIG=str(config_path), MARKETBRIDGE_API_ADDR=f"127.0.0.1:{port}")
        child = self.launch()
        self.ready(child, port)
        headers = {}
        if self.environment.get("MARKETBRIDGE_API_KEY"):
            headers["x-api-key"] = self.environment["MARKETBRIDGE_API_KEY"]
        with urlopen(Request(f"http://127.0.0.1:{port}/v1/system/outbox", headers=headers), timeout=5) as response:
            status = json.load(response)
        self.assertTrue(status["enabled"])
        self.assertEqual(status["status"]["pending_records"], 0)
        database = self.state / "outbox.sqlite"

        def cli(action, *paths, check=True):
            return subprocess.run([str(BINARY), "--outbox", action, str(database), *map(str, paths)],
                                  cwd=ROOT, env=self.environment, capture_output=True, text=True, check=check, timeout=15)

        self.assertNotEqual(cli("status", check=False).returncode, 0, "offline operations must not race the live owner")
        child.terminate()
        child.wait(timeout=15)
        offline = json.loads(cli("status").stdout)
        self.assertEqual(offline["origin"], status["status"]["origin"])
        exported = self.state / "pending.jsonl"
        self.assertEqual(json.loads(cli("export", exported).stdout)["exported_records"], 0)
        self.assertEqual(json.loads(cli("replay", exported).stdout)["replayed_records"], 0)

    @unittest.skipUnless(os.name == "posix", "Unix signal delivery check")
    def test_sigterm_and_sigint_seal_recordings(self):
        for shutdown_signal in (signal.SIGTERM, signal.SIGINT):
            with self.subTest(signal=shutdown_signal):
                with socket.socket() as probe:
                    probe.bind(("127.0.0.1", 0))
                    port = probe.getsockname()[1]
                recordings = self.state / f"recordings-{shutdown_signal}"
                self.environment.update(
                    MARKETBRIDGE_API_ADDR=f"127.0.0.1:{port}",
                    MARKETBRIDGE_RECORD_DIR=str(recordings),
                )
                child = self.launch()
                self.ready(child, port)
                child.send_signal(shutdown_signal)
                self.assertEqual(child.wait(timeout=15), 0)
                self.assertEqual(list(recordings.glob("*.partial")), [])
                sealed = list(recordings.glob("*.jsonl"))
                self.assertEqual(len(sealed), 1)
                result = subprocess.run(
                    [str(BINARY), "--verify-journal", str(sealed[0])],
                    cwd=ROOT, env=self.environment, check=True, capture_output=True, text=True,
                )
                verification = json.loads(result.stdout)
                self.assertTrue(verification["sealed"])
                self.assertEqual(verification["source_dropped_total"], 0)
                self.assertGreaterEqual(verification["records"], 1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=BINARY)
    args = parser.parse_args()
    BINARY = args.binary.resolve()
    if not BINARY.is_file():
        parser.error(f"build the binary first: {BINARY}")
    unittest.main(argv=[__file__], verbosity=2)
