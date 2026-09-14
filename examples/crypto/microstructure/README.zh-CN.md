# 加密微结构、逼空与清算

> **研究问题：** 可观察的流量、深度、OI、清算或波动率状态，在不假装知道交易者意图的前提下，
> 是否对应不同的后续价格响应？

## 案例地图

| 证据 | 入口 |
|---|---|
| Confluence 监控 | `short_squeeze_monitor.py`、`exhaustion_short_monitor.py`、`liquidation_reversal_monitor.py`、`crypto_microstructure_monitor.py` |
| 流量与深度 | `crypto_flow_book_confirmation.py`、`crypto_footprint_imbalance_*`、`crypto_spot_perp_depth_gap_*`、`crypto_liquidity_stress_*` |
| 双侧墙体 | `crypto_liquidity_sandwich_monitor.py`、`crypto_liquidity_sandwich_response_recorder.py`、`crypto_liquidity_sandwich_response_replay.py` |
| 清算研究 | `crypto_liquidation_burst_*`、`crypto_liquidation_price_cluster_*`、`crypto_liquidation_intensity_response_replay.py`、`liquidation_reversal_replay.py` |
| 事件/技术回放 | `crypto_cvd_divergence_replay.py`、`crypto_obv_divergence_response_replay.py`、`crypto_keltner_channel_response_replay.py`、`crypto_donchian_channel_response_replay.py`、`crypto_trade_imbalance_bar_replay.py`、`crypto_vpin_response_replay.py`、`crypto_*vwap*`、`crypto_*breakout*`、`crypto_*fair_value_gap*`、`crypto_session_*`、`crypto_weekly_rsi_cross_response_replay.py`、`crypto_weekday_hour_effect_replay.py` |
| 衍生品拥挤 | `crypto_taker_oi_response_replay.py`、`crypto_oi_price_divergence_response_replay.py`、`crypto_account_ratio_oi_response_replay.py`、`crypto_derivatives_*`、`crypto_adl_risk_*` |

Recorder/replay pair 会把状态与报价一起冻结，再测量固定记录窗口的有符号或绝对收益。
ADL pair 只把 Binance rating 当作提供方上下文，不证明发生了 ADL，也不推断私人账户风险。
实时清算存储现在按 venue/symbol 保留有界的近期事件窗口，不再覆盖上一条事件。给
`crypto_liquidation_burst_response_recorder.py` 传 `--source market` 可归档 Binance、Bybit、BitMEX、Gate
等已启用实时 feed；`--source history` 仍使用 OKX/CoinEx 有界历史路径。保留窗口不是完整历史账本，feed 缺口必须保留。
liquidity-sandwich pair 只检验一个更窄的公开 X 假设：当买卖两侧近盘口深度都明显、点差较窄时，
后续 BTC 绝对波动是否不同于普通快照；不会把显示深度称为持续墙体，也不推导区间交易机会。

`crypto_liquidity_sweep_response_replay.py` 检验公开“流动性扫损/收回”叙事中可以从 OHLCV 观察到的子集：当前 K 线刺破前序回看窗口的高点或低点，
随后收盘重新穿回该水平，并且实体/波动达到阈值；然后报告按方向对齐的未来收益。这不能证明真实止损流动性、潜在 liquidity pool、CISD、displacement 意图或可执行形态。

```bash
python3 examples/crypto/microstructure/crypto_liquidity_sweep_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 15m \
  --days 15 --lookback-bars 20 --sweep-buffer-bps 0 \
  --min-body-fraction 0.50 --min-range-bps 5 \
  --horizon-bars 8 --paper-cost-bps 10 --min-observations 5
```

研究线索来自 [KM Trading 在 X 的 setup 拆解](https://x.com/KMTrading_SMC/status/2032428981040103847)。
MarketBridge 的字段语义对照 [Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)，
并主动收窄 X 帖子术语：只检验前序区间突破、收回和 K 线几何，不把术语变成订单执行规则。

`crypto_atr_regime_response_replay.py` 是独立的波动率上下文研究：计算简单平均真实波幅（ATR），用当前时点以前的滚动分布把状态分为压缩、普通和扩张，
再比较各状态之后的有符号收益、绝对收益和路径风险。它不预测方向、不计算仓位、不设置止损，也不执行交易。研究线索来自
[X 上的 regime/ATR 讨论](https://x.com/viviennaBTC/status/2037854988442235187)，计算口径对照
[Binance Academy 的 ATR 说明](https://www.binance.com/en/square/post/510812)。

```bash
python3 examples/crypto/microstructure/crypto_atr_regime_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --atr-period 14 --regime-lookback 96 \
  --low-quantile 0.20 --high-quantile 0.80 \
  --horizon-bars 8 --min-observations 5
```

`crypto_breakout_retest_response_replay.py` 检验与 sweep 相反的延续假设：收盘突破前序区间后，在限定窗口内触及被突破水平，并重新收在突破方向一侧；未来窗口从回踩收盘开始，确保回踩先被观察再测量响应。
它不推断真实支撑/阻力、挂单、成交量确认或成交。研究线索来自
[Rekt Capital 在 X 的 BTC 突破/回踩讨论](https://x.com/rektcapital/status/1850982324621676715)，并对照
[Binance Academy 的加密突破说明](https://www.binance.com/en/academy/articles/a-beginners-guide-to-swing-trading-cryptocurrency)。

```bash
python3 examples/crypto/microstructure/crypto_breakout_retest_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --lookback-bars 24 --breakout-buffer-bps 2 \
  --retest-window 8 --retest-tolerance-bps 15 --horizon-bars 8 \
  --paper-cost-bps 10 --min-observations 5
```

`crypto_ichimoku_cloud_response_replay.py` 实现 point-in-time Ichimoku 响应表：同时观察价格相对云层的位置、Tenkan/Kijun 关系、云颜色和 Chikou 对比；当前可见的 Senkou 云值只读取位移以前已经计算出的历史线，避免把未来投影云层当成当前已知数据。
这些状态只是描述性分组，不包含预测、资金分配、止损或执行。研究线索来自未经验证的
[X 上 Ichimoku/云层讨论](https://x.com/Invst_Informant/status/2014788740992929906)，公式对照
[Binance Academy 的 Ichimoku 说明](https://www.binance.com/en/academy/articles/ichimoku-clouds-explained)。

```bash
python3 examples/crypto/microstructure/crypto_ichimoku_cloud_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --conversion-period 9 --base-period 26 \
  --span-b-period 52 --displacement 26 --horizon-bars 6 \
  --min-observations 5
```

`crypto_rsi_bollinger_extreme_response_replay.py` 是组合极值案例：把 RSI-only 极值、Bollinger-only 越界、共同超买/超卖以及普通 K 线分开，再比较后续响应。
它不假设极值必然反转；强趋势持续、指标口径和参数敏感性都会保留。研究线索来自
[X 上 BTC RSI + 上轨讨论](https://x.com/MichaelMOTTCM/status/1944846581611814956)，定义对照
[Binance RSI 词典](https://www.binance.com/en/academy/glossary/relative-strength-index)
和 [Bollinger Bands 说明](https://www.binance.com/en/square/post/42841)。

```bash
python3 examples/crypto/microstructure/crypto_rsi_bollinger_extreme_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --rsi-period 14 --band-period 20 --deviations 2 \
  --overbought 70 --oversold 30 --horizon-bars 12 \
  --min-observations 5
```

`crypto_fibonacci_retracement_response_replay.py` 只检验一个更窄的、point-in-time 回撤假设：
在当前 K 线以前的回看窗口内选择高点和低点，按两者的时间顺序确定上涨或下跌方向，
再把当前收盘价分到 38.2%、50% 或 61.8% 附近（另设区间内和区间外对照）。锚点窗口严格在当前 K 线之前结束，
因此不会把未来 swing 泄漏到特征；输出比较后续有符号、方向对齐和绝对收益。
它不声称 Fibonacci 水平必然是支撑/阻力，也不生成入场、止损或下单规则。研究线索来自
[X 上 WIF 的 Fibonacci 讨论](https://x.com/CryptoJournaal/status/2026693734063075685)，定义对照
[Binance Academy Fibonacci 指南](https://www.binance.com/en/academy/articles/a-guide-to-mastering-fibonacci-retracement)
和 [Binance 词典](https://www.binance.com/en/academy/glossary/fibonacci-retracement)。
如果窗口内最高点和最低点出现在同一根 K 线，方向不确定，结果会保留为 `missing_swing`，不会人为指定方向。

```bash
python3 examples/crypto/microstructure/crypto_fibonacci_retracement_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 90 --level-tolerance 0.03 \
  --horizon-bars 6 --min-observations 5
```

`crypto_fair_value_gap_response_replay.py` 研究独立的三根 K 线 imbalance 代理：第一根与第三根 wick 不重叠，
中间 K 线实体还必须达到显式比例；随后记录区域是 untouched、touched 还是 wick_filled，比较 bullish、bearish 和普通 K 线的响应。
它不声称看到了未成交量、机构意图、支撑/阻力或可成交价。研究线索来自
[X 上的 FVG 讨论](https://x.com/Bradgohtrades/status/2058684241958031814)，字段对照
[Binance Academy 蜡烛图说明](https://www.binance.com/en/square/post/492082)
和 [官方 K 线字段文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_fair_value_gap_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --min-gap-bps 5 --min-middle-body-fraction 0.50 \
  --horizon-bars 8 --min-observations 5
```

`crypto_obv_divergence_response_replay.py` 与 CVD 明确分开：它按经典规则在收盘上涨时给整根 K 线成交量加分、收盘下跌时减分，
再用回看窗口总成交量归一化，比较 bullish/bearish divergence、价格与 OBV 同向确认以及 mixed/缺失状态的后续响应。
它不观察主动成交方向、持仓归属、资金“积累”或鲸鱼意图；缺失 volume 会保留为 `missing_volume`，不会填零。
定义对照 [Binance 官方 OBV 说明](https://www.binance.com/en/square/post/1218711) 和
[Fidelity 的 OBV 公式与限制](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/OBV)。

```bash
python3 examples/crypto/microstructure/crypto_obv_divergence_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 20 --price-threshold-bps 20 \
  --obv-threshold 0.10 --horizon-bars 6 --min-observations 5
```

`crypto_keltner_channel_response_replay.py` 使用显式 EMA 中线和 trailing true-range ATR 通道宽度，
区分首次越过上/下轨、持续在轨外和通道内控制，再比较未来固定窗口的方向对齐、绝对和路径响应。
它不是平台私有指标复刻，也不是突破保证、反转规则、止损规则或执行模型。定义对照
[Binance Academy Keltner 对比说明](https://academy.binance.com/lt/articles/bollinger-bands-explained)
和 [Binance ATR/Keltner 教学](https://www.binance.com/pt/square/post/22339649998217)。

```bash
python3 examples/crypto/microstructure/crypto_keltner_channel_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --ema-period 20 --atr-period 10 --atr-multiplier 2 \
  --horizon-bars 8 --min-observations 5
```

`crypto_donchian_channel_response_replay.py` 只用当前 K 线以前的回看窗口最高/最低构造 Donchian channel，
区分首次越过上/下轨、持续在轨外和区间内控制，再比较未来固定窗口的方向对齐、绝对和路径响应。
它不把滚动极值称为真实支撑/阻力，也不生成趋势、止损或执行规则。字段对照
[Binance ATR/Keltner/Donchian 教学](https://www.binance.com/pt/square/post/22339649998217)
和 [官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_donchian_channel_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --lookback-bars 20 --horizon-bars 8 --min-observations 5
```

`crypto_supertrend_response_replay.py` 是透明的 ATR 趋势带响应研究：先用当前及前置 K 线的真实波幅计算 trailing simple ATR，
再用 K 线中点 ± multiplier 构造上下带，并只用当前/历史数据递推 clamp 后的有效带。收盘穿过前一方向的有效带时标记
`bullish_flip` 或 `bearish_flip`，其余有效 K 线作为 `bullish_trend` / `bearish_trend` 控制组，比较固定窗口的方向对齐、绝对和路径响应。
常见的 `atr-period=10`、`multiplier=3` 只是敏感性起点，不保证与 TradingView 或交易所私有平滑完全一致；这些标签不是入场、止损、预测或下单指令。
研究线索来自 [Binance Square 的 Supertrend 参数与读法说明](https://www.binance.com/en/square/post/24192142459218)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_supertrend_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --atr-period 10 --multiplier 3 --horizon-bars 8 \
  --min-observations 5
```

`crypto_adx_dmi_response_replay.py` 把公开的 ADX/DMI 叙事拆成可证伪的趋势强度研究：按明确的 Wilder 风格递推平滑真实波幅、+DM、-DM、+DI、-DI、DX 和 ADX，
再把每个时点分为 `strong_bullish`、`strong_bearish`、`strong_mixed`、弱方向状态或 `range_or_mixed`。+DI/-DI 方向改变会单独保留为
`bullish_di_cross` / `bearish_di_cross` 事件，并把强趋势与弱方向/区间控制组分开比较固定窗口的有符号、方向对齐和绝对响应。ADX 只描述历史方向性强度，不能证明延续、交易者意图、入场、止损或执行；周期 14 和阈值 25 只是敏感性起点。
研究线索来自 [Binance Square 的 ADX/DMI 说明](https://www.binance.com/en/square/post/15751696197586)，预热与平滑边界对照
[TradingView 的 DMI 计算说明](https://www.tradingview.com/support/solutions/43000502250-directional-movement-dmi/)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_adx_dmi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --strength-threshold 25 --horizon-bars 8 \
  --min-observations 5
```

`crypto_aroon_response_replay.py` 研究“最近一次创新高/低距今多久”是否对应不同的后续响应：使用包含当前 K 线的 point-in-time OHLC 回看窗口计算 Aroon Up/Down 及 oscillator，
把样本分为 `bullish_recent_extreme`、`bearish_recent_extreme`、`consolidation` 和 `balanced`，并单独保留 Aroon 方向交叉事件。
强的近期极值状态与盘整控制组比较固定窗口的有符号、方向对齐、绝对和路径响应。“趋势新鲜度”不是价格预测、入场、止损或执行规则；窗口、70/50 阈值以及相同高低点选择最近出现的规则都会显式输出。
公式边界对照 [TradingView 的 Aroon 说明](https://www.tradingview.com/support/solutions/43000501801-aroon-indicator/)，加密语义参考
[CoinMarketCap 的 Aroon 词典](https://coinmarketcap.com/academy/glossary/aroon-indicator/)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_aroon_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --trend-threshold 70 \
  --consolidation-threshold 50 --horizon-bars 8 --min-observations 5
```

`crypto_mfi_response_replay.py` 把“价格 + 成交量资金流”拆成可证伪的极值研究：用典型价乘以 K 线 volume 得到 raw money flow，按典型价相对上一根的变化分别累加正/负资金流，计算 0–100 的 MFI。
样本分为 `overbought`、`oversold`、`neutral`，并单独保留 `oversold_reclaim` / `overbought_rejection` 事件，再比较极值与中性控制组的固定窗口有符号、方向对齐、绝对和路径响应。
这里的 volume 是 K 线成交量，不是主动买卖、交易所净流、持仓归属或资金流向证明；80/20 阈值、周期和 horizon 只是敏感性参数，不生成反转、入场或执行规则。
公式对照 [TradingView 的 MFI 计算说明](https://www.tradingview.com/support/solutions/43000502348-money-flow-mfi/)，公开加密资金流线索参考
[Binance Square 的 money-flow 讨论](https://www.binance.com/en/square/post/21507916375097)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_mfi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --overbought 80 --oversold 20 \
  --horizon-bars 8 --min-observations 5
```

`crypto_cmf_response_replay.py` 与 MFI/OBV 明确分开：先按 `(close-low)/(high-low)` 计算每根 K 线的 money-flow multiplier，再乘以 volume，
用窗口内 money-flow volume 总和除以 volume 总和得到 CMF。样本分为 `positive_pressure`、`negative_pressure` 和 `neutral`，并单独保留
`positive_zero_cross` / `negative_zero_cross` 事件，再比较压力状态与中性控制组的固定窗口响应。`high == low` 的 K 线不会被填成压力，相关窗口保持缺失。
这是收盘位置加权的 OHLCV 代理，不是主动成交流、交易所净流、持仓归属或执行规则；周期、阈值和 horizon 都是敏感性参数。
公式对照 [TradingView 的 Chaikin Money Flow 说明](https://www.tradingview.com/support/solutions/43000501974-chaikin-money-flow-cmf/)，公开加密资金流线索参考
[Binance Square 的 money-flow 讨论](https://www.binance.com/en/square/post/21507916375097)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_cmf_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 21 --positive-threshold 0.05 \
  --negative-threshold -0.05 --horizon-bars 8 --min-observations 5
```

`crypto_parabolic_sar_response_replay.py` 按明确的 Wilder 风格递推 Extreme Point（EP）和 Acceleration Factor（AF），把每个 point-in-time K 线标记为 `bullish_trend` / `bearish_trend`，并保留 `bullish_flip` / `bearish_flip` 事件，与持续趋势控制组比较固定窗口响应。
虽然 SAR 名称来自 Stop and Reverse，本案例只研究状态，不提交止损、反转、入场或订单；初始方向、前两根 high/low clamp、`start-af`、`step` 和 `max-af` 都是显式参数。
公式与预热边界对照 [TradingView 的 Parabolic SAR 说明](https://www.tradingview.com/support/solutions/43000502597-parabolic-sar-sar/)，加密使用语境参考
[Binance Academy 的 Parabolic SAR 指南](https://www.binance.com/en/square/post/43032)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_parabolic_sar_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --start-af 0.02 --step 0.02 --max-af 0.20 \
  --horizon-bars 8 --min-observations 5
```

`crypto_stochastic_response_replay.py` 用当前收盘价相对包含当前 K 线的最高/最低区间计算 raw `%K`，再用连续 trailing SMA 得到平滑 `%K/%D`。
样本分为 `overbought`、`oversold`、`neutral`，并单独保留 `bullish_kd_cross` / `bearish_kd_cross`，比较极值与中性控制组的固定窗口有符号、方向对齐、绝对和路径响应。
zero-range 或不完整平滑窗口保持缺失，不跨缺口拼接；80/20、14/3/3 和 horizon 只是敏感性参数，不生成反转、入场、止损或执行规则。
公式对照 [TradingView 的 Stochastic 说明](https://www.tradingview.com/support/solutions/43000502332-stochastic-stoch/)，加密语境参考
[Binance 的超买/超卖说明](https://www.binance.com/en/square/post/684815)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_stochastic_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --k-period 14 --smooth-k 3 --smooth-d 3 \
  --overbought 80 --oversold 20 --horizon-bars 8 --min-observations 5
```

`crypto_stochrsi_response_replay.py` 先按明确的 Wilder 平滑计算 RSI，再把当前 RSI 放入自己的 trailing RSI 高低区间，最后用连续 SMA 得到 StochRSI `%K/%D`。
样本分为 `overbought`、`oversold`、`neutral`，并单独保留 `bullish_kd_cross` / `bearish_kd_cross`，比较极值与中性控制组的固定窗口响应。
它是“指标的指标”，比普通 RSI 更敏感；常数 RSI 窗口和不完整平滑保持缺失，不跨缺口拼接。80/20、14/14/3/3 和 horizon 只是敏感性参数，不生成反转、入场、止损或执行规则。
公式对照 [TradingView 的 StochRSI 说明](https://www.tradingview.com/support/solutions/43000502333-stochastic-rsi-stoch-rsi/)，加密语境参考
[Binance Academy 的 StochRSI 指南](https://www.binance.com/en/square/post/511150)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_stochrsi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --rsi-period 14 --stoch-period 14 \
  --smooth-k 3 --smooth-d 3 --overbought 80 --oversold 20 \
  --horizon-bars 8 --min-observations 5
```

`crypto_vortex_response_replay.py` 按当前/前一根 K 线计算 `VM+ = |high - prior low|`、`VM- = |low - prior high|`，再用相同窗口的真实波幅总和归一化得到 `VI+` / `VI-`。
样本分为 `bullish_pressure`、`bearish_pressure`、`balanced`，并单独保留 `bullish_vi_cross` / `bearish_vi_cross` 事件，比较方向压力与平衡控制组的固定窗口响应。
`minimum-spread`、周期和 horizon 都是敏感性参数；zero-range/缺失窗口保持不可用，不把交叉当成反转、入场、止损或执行规则。
公式对照 [TradingView 的 Vortex Indicator 说明](https://www.tradingview.com/support/solutions/43000591352-vortex-indicator/)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_vortex_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --minimum-spread 0.05 \
  --horizon-bars 8 --min-observations 5
```

`crypto_pivot_response_replay.py` 固定使用 UTC 日作为枢轴周期，用上一 UTC 日的 high/low/close 计算传统 `P`、`R1/S1`、`R2/S2`，然后给下一日每根 K 线标记 `r1_rejection`、`s1_reclaim`、`above_r1`、`below_s1`、`inside_pivot_range` 或 `near_pivot`。
事件与区间控制组比较固定窗口的有符号、方向对齐、绝对和路径响应。枢轴是 OHLC 水平代理，不是真实支撑/阻力；UTC 日界线、near tolerance 和 horizon 都是显式参数，不生成入场、止损或订单。
公式对照 [TradingView 的 Pivot Points Standard 说明](https://www.tradingview.com/support/solutions/43000521824-pivot-points-standard/)，加密日内语境参考
[Binance 的 Pivot/支撑阻力说明](https://www.binance.com/en/square/post/375172)，输入字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_pivot_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --tolerance-bps 10 --horizon-bars 8 --min-observations 5
```

`crypto_liquidation_intensity_response_replay.py` 是绝对清算 burst 和价格 cluster 案例的归一化 companion：
把观察到的清算名义额除以同一回看窗口内的 typical-price × base-volume 成交额代理，再比较高强度和普通窗口的后续绝对波动。
清算 venue、价格 venue 和覆盖元数据都会保留；side 只是提供方字段，有界历史也不是完整 cascade 账本。
比例化研究线索来自[清算感知回测框架](https://candlefeed.ai/blog/liquidation-aware-backtesting/)，事件语义对照
[Binance 清算流文档](https://developers.binance.com/en/docs/derivatives/coin-margined-futures/websocket-market-streams/Liquidation-Order-Streams)
和 [官方 K 线字段文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/microstructure/crypto_liquidation_intensity_response_replay.py \
  --liquidation-exchange okx --price-exchange okx --symbol BTCUSDT \
  --interval 5m --window-bars 12 --horizon-bars 12 \
  --min-intensity-ratio 0.01 --min-observations 3
```

`crypto_weekly_rsi_cross_response_replay.py` 是独立的收盘价研究：在 `1w` K 线上计算明确实现的
RSI(14) 与其 14 周简单均线，比较上穿/下穿后固定周数的有符号收益和路径最低收益。它不会继承平台私有指标口径，
也不会把 X 帖子里的回撤描述变成预测。

```bash
python3 examples/crypto/microstructure/crypto_weekly_rsi_cross_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1w --days 3650 \
  --rsi-period 14 --rsi-sma-period 14 --horizon-weeks 4 \
  --min-observations 3
```

## 快速开始

```bash
python3 examples/crypto/microstructure/crypto_adl_risk_monitor.py \
  --symbol BTCUSDT --exchange binance
python3 examples/crypto/microstructure/crypto_adl_risk_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 60 \
  --output work/crypto-adl-risk-response.jsonl
python3 examples/crypto/microstructure/crypto_adl_risk_response_replay.py \
  --input work/crypto-adl-risk-response.jsonl --horizon-records 3 \
  --min-observations 5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_monitor.py \
  --symbol BTCUSDT --exchange binance --depth-band-bps 10 \
  --min-side-depth-notional 100000 --min-symmetry-ratio 0.5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --output work/crypto-liquidity-sandwich-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_replay.py \
  --input work/crypto-liquidity-sandwich-response.jsonl \
  --horizon-records 3 --min-observations 5
```

完整命令保留在 [`README.md`](README.md)。首次轮询可能没有 OI 基线；清算 side、成交 side
和盘口语义都取决于提供方，必须保留在输出中。

OI/价格象限回放会把每根价格 K 线与不晚于该时间的最新 OI 对齐，保留 OI 年龄和提供方单位，
分类为价格上升/OI 上升、价格上升/OI 下降、价格下降/OI 上升、价格下降/OI 下降四种可观察状态，
再比较后续有符号和绝对收益。标签不证明逼空、新空头、长仓清算或交易者意图。

```bash
python3 examples/crypto/microstructure/crypto_oi_price_divergence_response_replay.py \
  --symbol BTCUSDT --price-exchange binance --oi-exchange okx \
  --price-interval 5m --oi-interval 5m --days 2 \
  --lookback-bars 3 --horizon-bars 3 --min-observations 5
```

分解动机来自 [TheCryptoData 的公开 OI 与清算讨论](https://x.com/TheCryptoData/status/1948466627365769584)，
字段语义同时参考[OKX 合约 OI 文档](https://www.okx.com/docs-v5/en/#rest-api-trading-data-get-contracts-open-interest-and-volume)
和现有 Binance 历史接口。它们只是研究输入，不证明 divergence 交易具有收益。

```bash
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_recorder.py \
  --source market --exchange binance --price-exchange binance \
  --symbol BTCUSDT --iterations 120 --interval-secs 30 \
  --output work/crypto-live-liquidation-burst-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_replay.py \
  --input work/crypto-live-liquidation-burst-response.jsonl \
  --window-hours 1 --horizon-records 12 --threshold-notional 1000000 \
  --cooldown-records 12 --min-observations 3
```

## 证据规则

- Confluence 分数只是筛选观察，不是进出场信号。
- OI、flow、liquidation 或报价缺失时输出 `observe_only`，不能填零。
- 滚动 buffer、K 线近似或 aggregate long/short 比例不能揭示持仓归属、意图、dealer sign、
  潜在清算价位或因果关系。
- 成本、资金费率、借贷、延迟、滑点、排队和成交都不会被静默推断。

出处与覆盖说明在原目录文档中维护，包括 Binance [ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk)
和[公开清算研究线索](https://x.com/angustias87/status/2039147109228925373)。

## 边界

本系列不下单、不执行清算、不签名钱包，也不把公开 aggregate 指标解释为用户私人账户状态。
