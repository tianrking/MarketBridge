# 预测市场研究

> 使用公开市场、订单簿、成交和结算数据做校准与时序研究；不下单、不签名钱包。

## 案例

- `polymarket_complement_monitor.py`：YES/NO 互补价格快照。
- `prediction_trade_flow.py`、`polymarket_trade_recorder.py`：有限公开成交流归档。
- `polymarket_price_shock_replay.py`、`polymarket_timing_replay.py`：描述性时序和延续性研究。
- `polymarket_settlement_replay.py`、`polymarket_calibration_report.py`：已结算结果评分、校准分箱、Brier 和 log loss。

完整命令见 [`../README.md`](../README.md)。每次回放都要写清市场身份、结算规则、观测起点、缺失成交、价格/手续费假设，
并明确公开成交不是私人成交账本。

## 边界

示例不提交、撤销或签名预测市场交易，只读取公开数据并生成有限纸面证据。
