#!/usr/bin/env python3
"""Summarize accept_pipeline evidence without changing raw measurements."""
import argparse
import json
import math
from pathlib import Path
import re


def histograms(text):
    groups = {}
    for line in text.splitlines():
        match = re.match(r'pipeline_latency_seconds_(bucket|sum|count)\{([^}]+)\} ([^ ]+)$', line)
        if not match:
            continue
        kind, label_text, value = match.groups()
        labels = dict(re.findall(r'(\w+)="([^"]*)"', label_text))
        stage = groups.setdefault(labels["stage"], {"count": 0.0, "sum": 0.0, "buckets": {}})
        if kind == "bucket":
            upper = float(labels["le"])
            stage["buckets"][upper] = stage["buckets"].get(upper, 0.0) + float(value)
        else:
            stage[kind] += float(value)
    result = {}
    for name, group in groups.items():
        def bound(fraction):
            upper = next((upper for upper, count in sorted(group["buckets"].items()) if count >= fraction * group["count"]), math.inf)
            return upper * 1000 if math.isfinite(upper) else None
        result[name] = {"count": int(group["count"]), "mean_ms": group["sum"] / group["count"] * 1000 if group["count"] else None,
                        "p95_bucket_upper_ms": bound(.95), "p99_bucket_upper_ms": bound(.99)}
    return result


def summarize(directory):
    report = json.loads((directory / "report.json").read_text(encoding="utf-8"))
    if not report["passed"]:
        raise ValueError("acceptance did not pass")
    samples = [json.loads(line) for line in (directory / "samples.jsonl").read_text(encoding="utf-8").splitlines()]
    final = samples[-1]
    recent = [s for s in samples if s["elapsed_seconds"] >= final["elapsed_seconds"] - 120]
    def resources(key):
        available = [s for s in samples if s[key].get("available")]
        if not available:
            return {"available": False}
        cpu = sum(max(0, b[key]["cpu_seconds"] - a[key]["cpu_seconds"]) for a, b in zip(available, available[1:]))
        tail = [s for s in recent if s[key].get("available")]
        rss = [s[key]["rss_bytes"] for s in available]
        return {"available": True, "peak_rss_bytes": max(rss), "last_rss_bytes": rss[-1],
                "mean_cpu_cores_sampled": cpu / (available[-1]["elapsed_seconds"] - available[0]["elapsed_seconds"]),
                "last_120s_rss_min_bytes": min(s[key]["rss_bytes"] for s in tail),
                "last_120s_rss_max_bytes": max(s[key]["rss_bytes"] for s in tail)}
    return {"passed": True, "events_before_restart": report["events_before_restart"],
            "requested_rate": json.loads((directory / "metadata.json").read_text(encoding="utf-8"))["arguments"]["rate"],
            "delivered_events_per_second_including_drain": report["events_before_restart"] / report["elapsed_ingestion_and_drain_seconds"],
            "server": resources("server"), "redis": resources("redis"),
            "stages": histograms((directory / "final-metrics.txt").read_text(encoding="utf-8")),
            "ready_200_samples": sum(s["ready_http_status"] == 200 for s in samples),
            "ready_503_samples": sum(s["ready_http_status"] == 503 for s in samples),
            "raw_history_capacity_loss_observed": any(s["memory"]["order_flow"]["quality"]["has_capacity_loss"] for s in samples),
            "last_memory": final["memory"], "http_states_p95_ms": report["state_http_p95_ms"],
            "notes": ["histogram percentiles are bucket upper bounds, not exact quantiles", "CPU is sampled process CPU, not host utilization", "Redis operation histogram includes empty-queue connection probes in this build", "No isolated maximum throughput or production SLA inferred"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.directory), ensure_ascii=False, indent=2))
