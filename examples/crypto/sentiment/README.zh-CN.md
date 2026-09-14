# 加密情绪与关注度

> **研究问题：** 公开情绪、新闻关注度或社交指标极值，是否比普通对齐观测呈现不同的后续 BTC 响应？

## 案例

- `crypto_sentiment_extremes_monitor.py` / recorder / replay：Alternative.me Fear & Greed，使用明确的记录条数窗口。
- `crypto_news_attention_monitor.py` / recorder / replay：有 canonical URL、发布时间的有限 CryptoPanic 新闻，比较非方向性波动。
- `crypto_social_signal_response_recorder.py` / replay：一个配置的 LunarCrush 或 Santiment 指标、其变化和 BTC 响应。

## 快速开始

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

默认值（`fear <= 20`、`greed >= 80`）都是可见参数。回放不会把 fear 变成“买入”、把 greed 变成“卖出”；
价格缺失、提供方中断、发布时间延迟和 proprietary score 量纲都要保留为缺口。

出处、新闻字段语义和完整命令见 [`README.md`](README.md)。公开 X 只是研究线索，不是业绩证据。

## 边界

示例不根据情绪交易、不抓取私人社区、不推断身份，也不把提供方 composite 指标当作通用情绪尺度。
