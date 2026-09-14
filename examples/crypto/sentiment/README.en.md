# Crypto sentiment and attention

> **Question:** do public sentiment, news-attention, or social-metric extremes
> have a different later BTC response than ordinary aligned observations?

## Cases

- `crypto_sentiment_extremes_monitor.py` / recorder / replay: Alternative.me
  Fear & Greed buckets with an explicit record-count horizon.
- `crypto_news_attention_monitor.py` / recorder / replay: bounded CryptoPanic
  items, canonical URLs, provider publication times, and non-directional move.
- `crypto_social_signal_response_recorder.py` / replay: one configured
  LunarCrush or Santiment metric, its change, and a BTC response.

## Quickstart

```bash
python3 examples/crypto/sentiment/crypto_sentiment_extremes_monitor.py \
  --symbol BTCUSDT --exchange binance --fear-max 20 --greed-min 80
python3 examples/crypto/sentiment/crypto_sentiment_extremes_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 86400 \
  --output work/crypto-sentiment-extremes.jsonl
python3 examples/crypto/sentiment/crypto_sentiment_extremes_replay.py \
  --input work/crypto-sentiment-extremes.jsonl --horizon-records 7 \
  --min-observations 5 --paper-cost-bps 20
```

The defaults (`fear <= 20`, `greed >= 80`) are caller-visible parameters.
The replay never turns fear into “buy” or greed into “sell”; missing prices,
provider outages, publication delay and proprietary score scales remain gaps.

See this guide for provenance, news-field semantics and commands. Public X
posts are research leads, not evidence of performance.

## Boundary

These examples do not trade on sentiment, scrape private communities, infer
identity, or claim that a provider composite is a universal sentiment scale.
