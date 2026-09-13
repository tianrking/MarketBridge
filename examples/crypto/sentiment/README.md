# Crypto sentiment extremes / 加密情绪极值

## English

This family tests a narrow, falsifiable question: after the public Fear &
Greed index enters an extreme fear or extreme greed bucket, does the selected
BTC price show a different fixed-record-horizon return distribution than the
ordinary aligned windows? `crypto_sentiment_extremes_monitor.py` is a snapshot
observer; the recorder freezes the signal and a MarketBridge price snapshot to
JSONL; the replay reports descriptive forward returns for the two extreme
states. It does not claim that fear means buy or greed means sell.

The default thresholds are `fear <= 20` and `greed >= 80`. The replay uses
`--horizon-records`, so a daily recorder with `--horizon-records 7` approximates
a one-week paper horizon. It requires `--min-observations` before reporting a
candidate verdict and keeps missing prices outside the aligned sample.

Provenance: the metric definition and API contract come from
[Alternative.me's Crypto Fear & Greed Index page and API](https://alternative.me/crypto/fear-and-greed-index/).
The public [BitcoinFear extreme-value post on X](https://x.com/BitcoinFear/status/2043554800046940376)
is treated only as a research lead. The index is a provider composite and the
X claim is not a performance claim. MarketBridge measures forward association
with explicit sample, timing and coverage limits.

## 中文

这一系列检验一个窄而可证伪的问题：公开 Fear & Greed 指数进入极度恐惧或极度贪婪区间后，
选定 BTC 价格在固定快照窗口的收益分布，是否与普通窗口不同？`crypto_sentiment_extremes_monitor.py`
只做单次观察；recorder 把情绪和 MarketBridge 价格快照写入 JSONL；replay 报告两个极值状态的描述性未来收益。
它不声称“恐惧就买、贪婪就卖”。

默认阈值是 `fear <= 20`、`greed >= 80`。回放使用 `--horizon-records`；如果每天记录一次，
`--horizon-records 7` 约等于一周纸面窗口。只有极值样本达到 `--min-observations` 才会报告候选，
缺少价格的记录会排除在对齐样本之外。

出处：指标定义和 API 契约来自 [Alternative.me Crypto Fear & Greed Index 页面及 API](https://alternative.me/crypto/fear-and-greed-index/)。
公开的 [BitcoinFear 极值 X 帖子](https://x.com/BitcoinFear/status/2043554800046940376) 只作为研究线索，
不是收益结论。该指数是提供方聚合指标，MarketBridge 只在明确样本量、时间对齐和覆盖限制的情况下测量未来关联。

## Commands / 命令

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
