# Crypto sentiment / 加密情绪与关注度

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

使用 Fear & Greed、有限新闻和 keyed social metrics，比较极值状态与普通窗口的后续 BTC 响应。
情绪分数是 provider composite，不是普适量纲，也不输出“恐惧买入/贪婪卖出”信号。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Fear & Greed | `crypto_sentiment_extremes_*` |
| News attention | `crypto_news_attention_*` |
| Social metrics | `crypto_social_signal_response_*` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/sentiment/crypto_sentiment_extremes_monitor.py \
  --symbol BTCUSDT --exchange binance --fear-max 20 --greed-min 80
```

recorder 用追加式 JSONL 冻结情绪与价格；replay 用 `--horizon-records` 明确记录窗口。
字段语义、来源和可选连接器见双语指南。

## Boundary / 边界

不抓取私人社区、不推断身份、不根据情绪自动交易；缺失价格、延迟和 provider 模型变化必须保留。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/sentiment/<entrypoint>.py`；参数、出处和限制见上述双语指南。
