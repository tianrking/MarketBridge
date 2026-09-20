# MarketBridge examples / 示例库

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

这是示例库的双语导航页。完整案例说明、运行参数、研究出处和限制分别维护在
[English guide](README.en.md) 与 [中文指南](README.zh-CN.md)；不要把本页当作策略信号列表。

## Library map / 目录地图

| Family / 系列 | Research focus / 研究重点 | Bilingual guide / 双语入口 |
|---|---|---|
| Crypto carry / 资金费率与套利 | basis、funding、跨 venue、三角价差 | [`English`](crypto/carry/README.en.md) · [`中文`](crypto/carry/README.zh-CN.md) |
| Crypto DeFi | pool flow、稳定币、路由冲击、流动性 | [`English`](crypto/defi/README.en.md) · [`中文`](crypto/defi/README.zh-CN.md) |
| Crypto macro / 宏观 | DXY、VIX、US10Y、ETF 流量、市场状态 | [`English`](crypto/macro/README.en.md) · [`中文`](crypto/macro/README.zh-CN.md) |
| Crypto microstructure / 微结构 | order flow、OI、清算、深度、波动率 | [`English`](crypto/microstructure/README.en.md) · [`中文`](crypto/microstructure/README.zh-CN.md) |
| Crypto on-chain / 链上 | transfers、mempool、矿工压力 | [`English`](crypto/onchain/README.en.md) · [`中文`](crypto/onchain/README.zh-CN.md) |
| Crypto options / 期权 | IV、skew、期限结构、gamma、VRP | [`English`](crypto/options/README.en.md) · [`中文`](crypto/options/README.zh-CN.md) |
| Crypto sentiment / 情绪 | Fear & Greed、新闻、社交指标 | [`English`](crypto/sentiment/README.en.md) · [`中文`](crypto/sentiment/README.zh-CN.md) |
| Crypto universe / 资产宇宙 | breadth、排名、配对、跨资产响应 | [`English`](crypto/universe/README.en.md) · [`中文`](crypto/universe/README.zh-CN.md) |
| Prediction markets / 预测市场 | 公开成交流、校准、结算回放 | [`English`](prediction/README.en.md) · [`中文`](prediction/README.zh-CN.md) |
| Weather / 天气 | 确定性观测与市场校准输入 | [`English`](weather/README.en.md) · [`中文`](weather/README.zh-CN.md) |

## Standard research loop / 统一研究流程

```text
MarketBridge API → monitor → recorder(JSONL) → replay → evidence review
```

1. 用 `config.research.yaml` 启动本地、只读的 MarketBridge 服务。
2. 先运行 `*_monitor.py`，确认 provider、时间戳、覆盖范围和缺失字段。
3. 用 `*_recorder.py` 记录追加式 JSONL；每个独立样本使用新的输出文件。
4. 用对应的 `*_replay.py` 指定前瞻窗口、最小样本量和纸面成本。
5. 把结果当作描述性证据：小样本、覆盖不足、提供方语义和未建模成本都必须保留。

```bash
MARKETBRIDGE_CONFIG=config.research.yaml cargo run
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance
```

## File roles / 文件职责

| Pattern / 文件模式 | Role / 职责 |
|---|---|
| `*_monitor.py` | 一次性读取当前 API 快照并打印结构化证据。 |
| `*_recorder.py` | 周期性冻结快照到追加式 JSONL。 |
| `*_replay.py` | 对归档或有界历史做固定窗口统计。 |
| `*_response_recorder.py` / `*_response_replay.py` | 将状态与同步 BTC 报价配对，测量后续响应。 |
| `strategy/` | Python 策略运行器与共享研究辅助代码。 |
| `tests/` | 确定性单元测试，不依赖实时行情。 |

## Boundary / 边界

示例是研究观察器，不是交易机器人。它们不会下单、撤单、改单、签名钱包、转移资金、
管理仓位或声称实盘账户 PnL。报价差不是成交，资金费率不是已实现收益，公开聚合数据也
不是私人账户状态。缺失值永远保持缺失，不会静默填零。

Rust 负责连接器、标准化、历史、缓存、质量元数据和 API；策略假设、记录、回放、测试和
双语研究文档优先使用 Python。

## Verification / 验证

```bash
PYTHONPATH=examples/crypto/options:examples/crypto/defi:examples/crypto/microstructure:examples/crypto/onchain:examples/crypto/carry:examples/crypto/macro:examples/crypto/universe:examples/crypto/sentiment:examples/crypto/strategy:examples/prediction:examples/weather \
  python3 -m unittest discover -s examples/tests -p 'test_*.py'
python3 -m compileall -q examples
```

新案例请先按系列归类，再在对应的中英文指南中记录假设、数据接口、出处和边界。
策略接入验收规则见
[`docs/user-guide/12-strategy-intake.md`](../docs/user-guide/12-strategy-intake.md)，开发证据见
[`docs/development-log.md`](../docs/development-log.md)。

<details>
<summary>Crypto entrypoint index / 加密案例入口索引</summary>

This list contains the runnable case scripts under `examples/crypto/`.
Compatibility launchers (`crypto/entrypoint.py` and the shared
`crypto/strategy/` runner) are documented separately above and are intentionally
not repeated as research cases.

- `basis_carry_monitor.py`
- `crypto_basis_recorder.py`
- `crypto_basis_replay.py`
- `crypto_coinbase_premium_historical_replay.py`
- `crypto_coinbase_premium_monitor.py`
- `crypto_coinbase_premium_response_recorder.py`
- `crypto_coinbase_premium_response_replay.py`
- `crypto_cross_venue_orderbook_monitor.py`
- `crypto_cross_venue_orderbook_recorder.py`
- `crypto_cross_venue_orderbook_replay.py`
- `crypto_cross_venue_orderbook_response_recorder.py`
- `crypto_cross_venue_orderbook_response_replay.py`
- `crypto_cross_venue_price_gap_replay.py`
- `crypto_funding_band_monitor.py`
- `crypto_funding_band_response_recorder.py`
- `crypto_funding_band_response_replay.py`
- `crypto_funding_carry_accrual_replay.py`
- `crypto_funding_interval_change_response_replay.py`
- `crypto_funding_cross_section_replay.py`
- `crypto_funding_oi_replay.py`
- `crypto_funding_regime_replay.py`
- `crypto_funding_spread_response_replay.py`
- `crypto_historical_basis_replay.py`
- `crypto_positioning_regime_replay.py`
- `crypto_predicted_funding_monitor.py`
- `crypto_premium_funding_response_replay.py`
- `crypto_triangular_arbitrage_monitor.py`
- `crypto_triangular_arbitrage_recorder.py`
- `crypto_triangular_arbitrage_replay.py`
- `crypto_triangular_arbitrage_response_recorder.py`
- `crypto_triangular_arbitrage_response_replay.py`
- `funding_convergence_monitor.py`
- `funding_convergence_replay.py`
- `funding_curve_demo.py`
- `funding_extremes.py`
- `crypto_defi_funding_yield_risk_monitor.py`
- `crypto_defi_funding_yield_risk_recorder.py`
- `crypto_defi_funding_yield_risk_replay.py`
- `crypto_defi_pool_flow_monitor.py`
- `crypto_defi_pool_flow_recorder.py`
- `crypto_defi_pool_flow_replay.py`
- `crypto_defi_pool_flow_response_recorder.py`
- `crypto_defi_pool_flow_response_replay.py`
- `crypto_defi_yield_context_monitor.py`
- `crypto_jupiter_route_impact_monitor.py`
- `crypto_jupiter_route_impact_recorder.py`
- `crypto_jupiter_route_impact_replay.py`
- `crypto_meteora_dlmm_monitor.py`
- `crypto_meteora_dlmm_recorder.py`
- `crypto_meteora_dlmm_replay.py`
- `crypto_meteora_dlmm_response_recorder.py`
- `crypto_meteora_dlmm_response_replay.py`
- `crypto_orca_whirlpool_monitor.py`
- `crypto_orca_whirlpool_recorder.py`
- `crypto_orca_whirlpool_replay.py`
- `crypto_orca_whirlpool_response_recorder.py`
- `crypto_orca_whirlpool_response_replay.py`
- `crypto_raydium_pool_concentration_monitor.py`
- `crypto_raydium_pool_concentration_recorder.py`
- `crypto_raydium_pool_concentration_replay.py`
- `crypto_raydium_pool_concentration_response_recorder.py`
- `crypto_raydium_pool_concentration_response_replay.py`
- `crypto_stablecoin_depeg_monitor.py`
- `crypto_stablecoin_depeg_recorder.py`
- `crypto_stablecoin_depeg_replay.py`
- `crypto_stablecoin_historical_response_replay.py`
- `crypto_stablecoin_liquidity_monitor.py`
- `crypto_stablecoin_liquidity_response_recorder.py`
- `crypto_stablecoin_liquidity_response_replay.py`
- `crypto_stablecoin_rotation_response_replay.py`
- `crypto_etf_flow_response_recorder.py`
- `crypto_etf_flow_response_replay.py`
- `crypto_liquidity_confirmation_monitor.py`
- `crypto_liquidity_confirmation_recorder.py`
- `crypto_liquidity_confirmation_replay.py`
- `crypto_liquidity_impulse_replay.py`
- `crypto_macro_context_monitor.py`
- `crypto_macro_context_recorder.py`
- `crypto_macro_context_replay.py`
- `crypto_market_regime_monitor.py`
- `crypto_market_regime_recorder.py`
- `crypto_market_regime_replay.py`
- `crypto_account_ratio_oi_response_replay.py`
- `crypto_adl_risk_monitor.py`
- `crypto_adl_risk_response_recorder.py`
- `crypto_adl_risk_response_replay.py`
- `crypto_adx_dmi_response_replay.py`
- `crypto_anchored_vwap_replay.py`
- `crypto_aroon_response_replay.py`
- `crypto_atr_regime_response_replay.py`
- `crypto_bollinger_squeeze_replay.py`
- `crypto_breakout_retest_response_replay.py`
- `crypto_cci_response_replay.py`
- `crypto_cmf_response_replay.py`
- `crypto_cvd_divergence_replay.py`
- `crypto_24h_rollout_response_replay.py`
- `crypto_short_squeeze_reversal_monitor.py`
- `crypto_binance_short_opportunity_scanner.py`
- `crypto_derivatives_crowding_response_recorder.py`
- `crypto_derivatives_crowding_response_replay.py`
- `crypto_derivatives_sentiment_monitor.py`
- `crypto_derivatives_sentiment_recorder.py`
- `crypto_derivatives_sentiment_replay.py`
- `crypto_donchian_channel_response_replay.py`
- `crypto_fair_value_gap_response_replay.py`
- `crypto_fibonacci_retracement_response_replay.py`
- `crypto_flow_book_confirmation.py`
- `crypto_footprint_imbalance_monitor.py`
- `crypto_footprint_imbalance_recorder.py`
- `crypto_footprint_imbalance_replay.py`
- `crypto_footprint_response_recorder.py`
- `crypto_footprint_response_replay.py`
- `crypto_heikin_ashi_response_replay.py`
- `crypto_ichimoku_cloud_response_replay.py`
- `crypto_keltner_channel_response_replay.py`
- `crypto_liquidation_burst_replay.py`
- `crypto_crowded_liquidation_reversal_replay.py`
- `crypto_forced_vs_voluntary_liquidation_replay.py`
- `crypto_liquidation_burst_response_recorder.py`
- `crypto_liquidation_burst_response_replay.py`
- `crypto_liquidation_intensity_response_replay.py`
- `crypto_liquidation_price_cluster_replay.py`
- `crypto_liquidation_price_cluster_response_recorder.py`
- `crypto_liquidation_price_cluster_response_replay.py`
- `crypto_liquidity_sandwich_monitor.py`
- `crypto_liquidity_sandwich_response_recorder.py`
- `crypto_liquidity_sandwich_response_replay.py`
- `crypto_liquidity_stress_monitor.py`
- `crypto_liquidity_stress_recorder.py`
- `crypto_liquidity_stress_replay.py`
- `crypto_liquidity_stress_response_recorder.py`
- `crypto_liquidity_stress_response_replay.py`
- `crypto_liquidity_sweep_response_replay.py`
- `crypto_mfi_response_replay.py`
- `crypto_microstructure_monitor.py`
- `crypto_microstructure_response_recorder.py`
- `crypto_microstructure_response_replay.py`
- `crypto_obv_divergence_response_replay.py`
- `crypto_oi_impulse_response_recorder.py`
- `crypto_oi_impulse_response_replay.py`
- `crypto_oi_price_divergence_response_replay.py`
- `crypto_opening_range_breakout_response_replay.py`
- `crypto_parabolic_sar_response_replay.py`
- `crypto_pivot_response_replay.py`
- `crypto_profile_vwap_oi_response_replay.py`
- `crypto_quarter_hour_flow_replay.py`
- `crypto_rsi_bollinger_extreme_response_replay.py`
- `crypto_session_filter.py`
- `crypto_session_momentum_replay.py`
- `crypto_short_squeeze_response_recorder.py`
- `crypto_short_squeeze_response_replay.py`
- `crypto_spot_perp_depth_gap_monitor.py`
- `crypto_spot_perp_depth_gap_recorder.py`
- `crypto_spot_perp_depth_gap_replay.py`
- `crypto_spot_perp_depth_gap_response_recorder.py`
- `crypto_spot_perp_depth_gap_response_replay.py`
- `crypto_stochastic_response_replay.py`
- `crypto_stochrsi_response_replay.py`
- `crypto_supertrend_response_replay.py`
- `crypto_taker_oi_response_replay.py`
- `crypto_trade_imbalance_bar_replay.py`
- `crypto_volatility_breakout_replay.py`
- `crypto_volume_profile_breakout_replay.py`
- `crypto_vortex_response_replay.py`
- `crypto_vpin_response_replay.py`
- `crypto_vwap_deviation_reversion_replay.py`
- `crypto_weekday_hour_effect_replay.py`
- `crypto_weekend_gap_response_replay.py`
- `crypto_weekly_rsi_cross_response_replay.py`
- `exhaustion_short_monitor.py`
- `liquidation_reversal_monitor.py`
- `liquidation_reversal_replay.py`
- `liquidity_stress_monitor.py`
- `short_squeeze_monitor.py`
- `crypto_hash_ribbon_response_replay.py`
- `crypto_mayer_multiple_response_replay.py`
- `crypto_onchain_mempool_pressure_monitor.py`
- `crypto_onchain_mempool_pressure_recorder.py`
- `crypto_onchain_mempool_pressure_replay.py`
- `crypto_onchain_mining_pressure_monitor.py`
- `crypto_onchain_mining_pressure_recorder.py`
- `crypto_onchain_mining_pressure_replay.py`
- `crypto_onchain_transfer_burst_replay.py`
- `crypto_onchain_transfer_response_recorder.py`
- `crypto_onchain_transfer_response_replay.py`
- `crypto_deribit_cross_asset_volatility_response_replay.py`
- `crypto_deribit_volatility_index_response_replay.py`
- `crypto_deribit_volatility_index_vrp_response_replay.py`
- `crypto_historical_volatility_response_replay.py`
- `crypto_options_bull_call_spread_monitor.py`
- `crypto_options_bull_call_spread_recorder.py`
- `crypto_options_bull_call_spread_replay.py`
- `crypto_options_bull_call_spread_response_recorder.py`
- `crypto_options_bull_call_spread_response_replay.py`
- `crypto_options_gamma_monitor.py`
- `crypto_options_gamma_recorder.py`
- `crypto_options_gamma_replay.py`
- `crypto_options_gamma_response_recorder.py`
- `crypto_options_gamma_response_replay.py`
- `crypto_options_max_pain_monitor.py`
- `crypto_options_max_pain_recorder.py`
- `crypto_options_max_pain_replay.py`
- `crypto_options_max_pain_response_recorder.py`
- `crypto_options_max_pain_response_replay.py`
- `crypto_options_panic_regime_response_replay.py`
- `crypto_options_put_call_oi_monitor.py`
- `crypto_options_put_call_oi_recorder.py`
- `crypto_options_put_call_oi_replay.py`
- `crypto_options_put_call_oi_response_recorder.py`
- `crypto_options_put_call_oi_response_replay.py`
- `crypto_options_skew_monitor.py`
- `crypto_options_skew_recorder.py`
- `crypto_options_skew_replay.py`
- `crypto_options_skew_response_recorder.py`
- `crypto_options_skew_response_replay.py`
- `crypto_options_skew_vol_regime_response_replay.py`
- `crypto_options_term_structure_replay.py`
- `crypto_options_term_structure_response_replay.py`
- `crypto_options_vrp_monitor.py`
- `crypto_options_vrp_recorder.py`
- `crypto_options_vrp_replay.py`
- `crypto_options_vrp_response_replay.py`
- `crypto_news_attention_monitor.py`
- `crypto_news_attention_recorder.py`
- `crypto_news_attention_replay.py`
- `crypto_sentiment_extremes_monitor.py`
- `crypto_sentiment_extremes_recorder.py`
- `crypto_sentiment_extremes_replay.py`
- `crypto_social_signal_response_recorder.py`
- `crypto_social_signal_response_replay.py`
- `crypto_adaptive_cross_asset_replay.py`
- `crypto_altcoin_breadth_replay.py`
- `crypto_cross_asset_correlation_response_replay.py`
- `crypto_cross_asset_lead_lag_response_replay.py`
- `crypto_cross_asset_momentum_replay.py`
- `crypto_drawdown_recovery_response_replay.py`
- `crypto_global_market_regime_monitor.py`
- `crypto_global_market_regime_recorder.py`
- `crypto_global_market_regime_replay.py`
- `crypto_pairs_mean_reversion_replay.py`
- `crypto_trend_template_response_replay.py`
- `crypto_universe_delist_risk_monitor.py`
- `crypto_universe_opportunity_recorder.py`
- `crypto_universe_opportunity_replay.py`
- `crypto_universe_opportunity_response_recorder.py`
- `crypto_universe_opportunity_response_replay.py`
- `crypto_universe_opportunity_scan.py`
- `crypto_volatility_adjusted_momentum_replay.py`
- `crypto_volatility_adjusted_momentum_sweep.py`
- `crypto_volatility_adjusted_momentum_walkforward.py`

</details>
