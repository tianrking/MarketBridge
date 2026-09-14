# Python strategy runner / Python 策略运行器

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

This directory contains the shared Python entry point for non-engineers. It
calls read-only MarketBridge endpoints, applies a small, inspectable hypothesis
function, and prints structured evidence. Rust remains responsible for data
collection, normalization, history, cache and API behavior.

## What belongs here / 本目录职责

| File / 文件 | Role / 作用 |
|---|---|
| `python_strategy_runner.py` | One CLI for the supported Python research strategies. |
| `strategy_entrypoint.py` | Compatibility launcher for embedding the runner. |
| `README.en.md` / `README.zh-CN.md` | Full commands, inputs and limitations. |

The runner is a convenience layer, not a second data model. For a focused
case, prefer the family-specific script and bilingual guide linked from
[`../../README.md`](../../README.md).

## Boundary / 边界

The runner never places, cancels or replaces orders; signs wallets; moves
funds; manages positions; or claims live-account PnL. Missing data remains
missing, and every result is research evidence rather than an execution signal.
