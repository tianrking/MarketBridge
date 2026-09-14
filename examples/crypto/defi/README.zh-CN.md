# 加密 DeFi 流量与流动性研究

> **研究问题：** 池活动、稳定币状态或路由器冲击，是否构成可持续的流动性压力上下文？

## 案例地图

- `crypto_defi_pool_flow_*`：DEX swap volume 相对池报告流动性；response 版本把状态
  与 BTC 报价配对。
- `crypto_jupiter_route_impact_*`：配置报价规模阶梯和路由器报告的 impact；不是 swap 模拟器。
- `crypto_meteora_dlmm_*`、`crypto_orca_whirlpool_*`、`crypto_raydium_pool_concentration_*`：
  各 venue 的池快照、换手、fee/TVL、dynamic fee 和集中度上下文。
- `crypto_stablecoin_depeg_*`、`crypto_stablecoin_rotation_response_replay.py`：
  稳定币偏离及后续 BTC 响应诊断。
- `crypto_stablecoin_liquidity_*` 与 `crypto_stablecoin_liquidity_response_*`：
  DefiLlama 供应量快照和供应变化响应研究。
- `crypto_defi_funding_yield_risk_*`、`crypto_defi_yield_context_monitor.py`：
  保留提供方和覆盖字段的收益/资金费率上下文。

## 推荐流程

```bash
python3 examples/crypto/defi/crypto_defi_pool_flow_monitor.py \
  --symbol SOLUSDC --min-turnover-ratio 1.0
python3 examples/crypto/defi/crypto_defi_pool_flow_recorder.py \
  --symbol SOLUSDC --iterations 30 --interval-secs 60 \
  --output work/crypto-defi-pool-flow.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_replay.py \
  --input work/crypto-defi-pool-flow.jsonl --min-run 3
```

需要 BTC 响应研究时，使用对应的 `*_response_recorder.py` 和 `*_response_replay.py`。
完整命令矩阵和提供方出处见 [`README.md`](README.md)。

## 证据契约

池 volume、TVL、fees、供应量、route impact 和稳定币价格都是提供方字段，不等于 LP 收益、
无常损失、偿付能力、因果流量、MEV、gas 或可执行深度。分页、过期快照、缺少 mint、提供方
模型变更都必须保留；上游分页不完整或被拦截时输出 `observe_only`，不能声称流动性为零。

## 边界

DeFi 示例不连接钱包、不签名交易、不构造 swap 路由、不保证成交，也不管理 LP 仓位；只做只读
上下文和回放。
