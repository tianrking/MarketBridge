# Python 策略运行器

> **用途：** 为非工程师提供统一的 Python 研究入口，同时保持 Rust 作为
> MarketBridge 的数据与 API 层。

## 快速开始

在仓库根目录启动只读研究服务：

```bash
MARKETBRIDGE_CONFIG=config.research.yaml cargo run
```

再在另一个终端运行一个策略：

```bash
PYTHONPATH=examples/crypto/options:examples/crypto/carry:examples/crypto/microstructure:examples/crypto/universe \
  python3 examples/crypto/strategy/python_strategy_runner.py \
  --strategy funding_convergence --symbol BTCUSDT \
  --funding-exchanges binance,bybit,okx
```

查看可用策略和参数：

```bash
python3 examples/crypto/strategy/python_strategy_runner.py --help
```

## 如何阅读输出

JSON 输出会保留提供方行、时间戳、新鲜度/覆盖字段；证据不完整时明确输出
`observe_only`。运行器不会把缺失观测填成零，也不会把报价差、资金费率或聚合 OI
直接转换成已实现收益。

需要深入研究时，应回到各分类目录的 monitor、recorder 和 replay；入口见
[`../README.zh-CN.md`](../README.zh-CN.md)。这些案例会更明确地暴露假设、窗口、
最小样本量和纸面成本。

## 文件职责

- `python_strategy_runner.py`：共享 CLI 和策略分发器。
- `strategy_entrypoint.py`：为需要稳定启动路径的调用者提供兼容包装。

## 边界

本目录只读研究，不下单、撤单、改单，不签名钱包、不转移资金、不管理仓位，也不报告
实盘账户 PnL。Rust 负责连接器、标准化、存储和 API；Python 负责假设与解释层。
