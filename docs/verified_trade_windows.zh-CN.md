# 有限成交窗口恢复

对 `[start_ms,end_ms)` 的成交数量、买卖量与名义金额进行重建，而不是清除全局缺口。当前支持 Binance aggTrade 原生连续 ID 和明确标注的合成验收源。

```text
GET /v1/market/order-flow/verified-window?exchange=binance&market=perp&symbol=BTCUSDT&start_ms=...&end_ms=...
market-bridge --recover-trade-window sealed.jsonl request.json
```

请求 JSON 使用上述五个字段。HTTP 从当前保留的原始成交验证；CLI 流式读取整个 CRC 有效、已封口的标准化 journal。服务端不接受文件路径。HTTP 验证成功返回 200，证据不足返回 422 且 `recovered=null`；非法范围/不支持协议返回 400。CLI 验证失败退出非零。

必须找到起点前一笔与终点及以后一笔边界成交，检查两者之间原生 ID 连续、源时间不倒退、价格/数量/方向有效。完全相同的相邻重复 ID 合并，内容冲突拒绝；没有内部成交时也需要连续的两笔边界。范围外早先跳号不妨碍一个较晚且独立闭合的窗口；内部缺口、未闭合窗口、溢出和损坏文件均不能获得恢复数值。

结果同时保留 source/subscriber/retention 或 journal 的全局损失证据。`lifetime_cvd_repaired=false`，`cvd_anchor=window_start_only`，不把局部重建当成终身 CVD 修复。证明建立在适配器正确保留原生 ID/时间的假设上；标准化文件与 CRC 不是交易所签名，也不能认证不支持协议的源覆盖。

原始成交使用跨标的全局条数/字节预算；条数压力滑动删除最老成交，字节压力分块收缩 deque，保留分配余量以避免每笔 grow/shrink。标的键数超预算才整键淘汰。去除了任意单标的 20,000 条上限；持续热点不会在达到全局条数上限时反复清空整段近期历史。被淘汰边界的窗口仍然无法验证。
