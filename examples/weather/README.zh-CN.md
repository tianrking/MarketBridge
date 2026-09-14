# 天气观测研究

> 使用确定性的 Open-Meteo 观测，作为事件研究和市场校准的明确输入。

## 案例

- `weather_event_observer.py`：标准化每日观测/预测 bucket。
- `weather_pressure_differential.py`：天气更新与对应公开市场报价对照，只生成调查候选。
- `weather_market_calibration.py`：把调用者维护的 JSONL manifest 与已验证的关闭市场结果连接。

地点、时区、观测时间、预测版本、市场身份和结算规则由调用者负责；示例不会仅凭天气推导概率。

命令见 [`../README.md`](../README.md)，原双语概览见 [`README.md`](README.md)。

## 边界

不执行天气衍生品或预测市场订单，只输出确定性观测和校准证据。
