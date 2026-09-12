"""Exercise every archived model with synthetic evidence against a running API.

Run from repository root; writes immutable synthetic runs to the configured workspace.
No market subscriptions or orders. Outputs run IDs so every result is auditable.
"""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import sys
import uuid
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk" / "python"))
from marketbridge import MarketBridge


def scenarios():
    root = Path(__file__).resolve().parents[1]
    e = json.loads((root / "examples/research/same-asset.json").read_text(encoding="utf-8"))
    a, b = deepcopy(e["buy"]["instrument"]), deepcopy(e["sell"]["instrument"])
    frame = {"evidence": e, "size_index": 0, "buy_fill_fraction": 1, "sell_fill_fraction": 1}
    values = {
        "same-asset-spot/v1": e,
        "candidate-screen/v1": {"as_of_ms": e["as_of_ms"], "min_net_bps": 0, "candidates": [{"id": "synthetic", "evidence": e}]},
        "scenario-replay/v1": {"frames": [e]},
        "prefunded-taker-scenario/v1": {"initial": {"buy_venue_quote": 1000000, "buy_venue_base": 0, "sell_venue_quote": 0, "sell_venue_base": 10}, "frames": [frame]},
        "allocated-spot-portfolio/v1": {"accounts": [{"instrument": a, "base": 0, "quote": 1000000}, {"instrument": b, "base": 10, "quote": 0}], "steps": [{**frame, "id": "entry", "purpose": "entry", "gate": {"kind": "always"}}]},
    }
    future = {**b, "product_type": "future", "expiry_ms": e["as_of_ms"] + 365 * 86400000, "contract_multiplier": 100}
    relation = {**e["relationship"], "kind": "hedge"}
    point = lambda instrument, price: {"instrument": instrument, "price": price, "known_at_ms": 10000, "source_time_ms": 10000, "evidence": "synthetic fixture, not market data"}
    basis = {"model": "spot-derivative-basis/v1", "as_of_ms": e["as_of_ms"], "max_age_ms": 1000, "max_skew_ms": 100, "left": point(a, 100), "right": point(future, 102), "relationship": relation, "carry_cost_per_unit": 1}
    values["spot-derivative-basis/v1"] = basis
    values["unit-premium/v1"] = {**basis, "model": "unit-premium/v1", "right": point(b, 102)}
    leg = lambda instrument, rate, interval: {"instrument": {**instrument, "product_type": "perp"}, "rate": rate, "interval_ms": interval, "known_at_ms": 10000, "evidence": "synthetic fixture"}
    values["funding-rate-comparison/v1"] = {"as_of_ms": e["as_of_ms"], "max_age_ms": 1000, "long": leg(a, 0.0008, 28800000), "short": leg(b, 0.0001, 3600000)}
    event = {"id": "synthetic-event", "source": "fixture", "source_url": "https://example.test/news", "title": "Synthetic news", "category": "listing", "instrument_ids": [a["id"]], "known_at_ms": 10000}
    values["announcement-window/v1"] = {"announcement": event, "instrument_id": a["id"], "as_of_ms": 11000, "horizon_ms": 1000, "max_gap_ms": 100, "samples": [{"time_ms": 9999, "known_at_ms": 9999, "price": 100, "evidence": "before"}, {"time_ms": 11000, "known_at_ms": 11000, "price": 101, "evidence": "after"}]}
    return values


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--export", type=Path, help="create a new directory of runnable synthetic model JSON inputs, without network")
    args = parser.parse_args()
    if args.export:
        args.export.mkdir(parents=True, exist_ok=False)
        for model, inputs in scenarios().items():
            with (args.export / (model.replace("/", "-") + ".json")).open("x", encoding="utf-8") as file:
                json.dump(inputs, file, ensure_ascii=False, indent=2, allow_nan=False)
        print(f"Exported {len(scenarios())} synthetic model inputs to {args.export}")
    else:
        client = MarketBridge(args.base_url, api_key=os.environ.get("MARKETBRIDGE_API_KEY"))
        for model, inputs in scenarios().items():
            run_id = "all-models-" + uuid.uuid4().hex
            run = client.run(run_id, model, inputs)
            result = run["payload"]["output"]
            assert result["status"] == "completed", (model, result)
            if model in {"spot-derivative-basis/v1", "unit-premium/v1", "funding-rate-comparison/v1", "announcement-window/v1"}:
                assert result["result"]["conditional_net_profit"] is None
            restored = client.workspace("get", {"namespace": "runs", "id": run_id})
            assert restored["document"]["payload"] == run["payload"]
            print(f"PASS {model}: {run_id}")
