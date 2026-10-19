#!/usr/bin/env python3
"""Independently verify the committed local native-protocol acceptance evidence.

Checks hashes, durable row CRCs, original JSON normalization, native continuity,
finite-window CRC and totals. These checks do not authenticate an exchange.
"""
import argparse
from decimal import Decimal
import gzip
import hashlib
import json
from pathlib import Path
import struct
import zlib


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(directory):
    proof = json.loads((directory / "recovered-window.json").read_text())
    quality = proof["window"]["quality"]
    scope = quality["scope"]
    # Rust WindowRequest uses declared struct field order, not sorted JSON keys.
    ordered = {k: scope[k] for k in ("exchange", "market", "symbol", "start_ms", "end_ms")}
    checksum = zlib.crc32(json.dumps(ordered, separators=(",", ":")).encode())
    previous = before = after = None
    count = 0
    buy = sell = buy_notional = sell_notional = Decimal(0)
    with gzip.open(directory / "verified-native-records.jsonl.gz", "rt", encoding="utf-8") as stream:
        for line in stream:
            record = json.loads(line)
            expected = zlib.crc32(struct.pack("<QQ", record["job_id"], record["sequence"]) + record["body"].encode())
            require(expected == record["crc32"], "durable native row CRC mismatch")
            row = json.loads(record["body"])
            native = row["native"]
            require(row["sequence"] == record["sequence"] == native["a"] and row["ts_ms"] == native["T"]
                    and row["price"] == float(native["p"]) and row["qty"] == float(native["q"])
                    and row["buy"] == (not native["m"]), "normalization differs from original native JSON")
            require(row["price"] > 0 and row["qty"] > 0, "invalid native economics")
            if previous:
                require(row["sequence"] == previous["sequence"] + 1 and row["ts_ms"] >= previous["ts_ms"], "native gap or time regression")
            previous = row
            checksum = zlib.crc32(struct.pack("<QQddB", row["sequence"], row["ts_ms"], row["price"], row["qty"], 1 if row["buy"] else 2), checksum)
            if row["ts_ms"] < scope["start_ms"]:
                require(before is None, "fixture export must start at the chosen predecessor")
                before = row["sequence"]
            elif row["ts_ms"] >= scope["end_ms"]:
                after = row["sequence"]
            else:
                count += 1
                qty, price = Decimal(native["q"]), Decimal(native["p"])
                if row["buy"]:
                    buy += qty
                    buy_notional += qty * price
                else:
                    sell += qty
                    sell_notional += qty * price
    totals = proof["window"]["recovered"]
    require(before == quality["before_sequence"] and after == quality["after_sequence"], "native boundaries differ")
    require(checksum == quality["evidence_crc32"], "finite window evidence CRC mismatch")
    require(count == totals["trade_count"] and buy == Decimal(str(totals["buy_qty"]))
            and sell == Decimal(str(totals["sell_qty"])) and buy_notional == Decimal(str(totals["buy_notional"]))
            and sell_notional == Decimal(str(totals["sell_notional"])), "native ledger totals differ")
    require(not proof["global_quality_cleared"] and not quality["lifetime_cvd_repaired"], "finite proof cleared global loss")
    return {"mode": directory.name, "verified_trades": count, "window_crc32": checksum}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    root = args.directory.resolve()
    manifest = json.loads((root / "manifest.sha256.json").read_text())
    for name, checksum in manifest.items():
        path = (root / name).resolve()
        require(path.is_relative_to(root), "manifest path escapes evidence directory")
        require(hashlib.sha256(path.read_bytes()).hexdigest() == checksum, "artifact SHA256 mismatch: " + name)
    print(json.dumps({"passed": True, "hashes_checked": len(manifest), "windows": [verify(root / mode) for mode in ("compact", "redis")]}))


if __name__ == "__main__":
    main()
