# Crypto universe and cross-asset research

> **Question:** can transparent, point-in-time candidate rankings or relative
> value states be tested without becoming a portfolio allocator?

## Case map

- `crypto_universe_opportunity_scan.py` / recorder / replay / response pair:
  liquidity, realized volatility and funding candidate persistence.
- `crypto_cross_asset_momentum_replay.py`,
  `crypto_volatility_adjusted_momentum_*`, and
  `crypto_adaptive_cross_asset_replay.py`: ranking, sensitivity and chronological
  holdout studies.
- `crypto_altcoin_breadth_replay.py` and `crypto_global_market_regime_*`:
  participation and provider-level global-market context.
- `crypto_pairs_mean_reversion_replay.py`: fixed-parameter spread deviation and
  convergence diagnostic.
- `crypto_universe_delist_risk_monitor.py`: missing/stale quote data-quality guard.
- `crypto_trend_template_response_replay.py`: a price-only 50/150/200-day
  trend-template response study with a trailing 52-week range filter.

## Quickstart

```bash
python3 examples/crypto/universe/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m --min-score 2
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_walkforward.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 \
  --train-fraction 0.7 --roundtrip-cost-bps 20
```

Every selection is a point-in-time paper list. Missing constituents, delisted
identity, look-ahead, universe definition, equal-weight assumptions and cost
hurdles must be explicit. “Top-k” is not an allocation instruction or a Sharpe
guarantee; the breadth case is an approximation, not an official index.

The complete command matrix and provenance links are in [`README.md`](README.md).

```bash
python3 examples/crypto/universe/crypto_trend_template_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1d --days 1825 \
  --sma-short-days 50 --sma-medium-days 150 --sma-long-days 200 \
  --slope-days 22 --range-days 252 --horizon-days 30 \
  --min-observations 5
```

## Boundary

This family does not rebalance capital, route orders, manage positions, or
pretend that a historical ranking is tradable capacity.
