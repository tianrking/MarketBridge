# 07 · 公告事件与事件窗口研究

## 自动接收和手工导入

启用扫描配置的 `record_announcements` 后，程序订阅现有 external signal 事件流，
从同时带 title/url 的事件中保存公告。具体新闻接口仍由原有 sentiment/custom API
连接器配置负责；此开关不自动订阅全球交易所公告或付费新闻。

自动记录按 source/title/url 去重，保留首次观察时间；没有凭据时 published_at_ms 为 null。
instrument_ids 初始为空，因为新闻里的 BTC 字样不能自动证明某个交易合约受影响。
来源变更标题会生成新记录。队列缺口会写日志，不能声称新闻历史完整。

手工或自己的公开源适配程序用 workspace `announcement_put`：

```json
{"action":"announcement_put","request":{
  "id":"exchange-notice-001","source":"operator-verified-source",
  "source_url":"https://example.test/notices/001",
  "title":"合成公告示例","category":"listing",
  "instrument_ids":["venue-a-btc"],
  "published_at_ms":10000,"known_at_ms":10020,"effective_at_ms":20000
}}
```

这是合成说明，不是真实新闻。替换 URL、时间及已登记的标的 ID 后使用。
同一 ID 相同内容幂等；修订用新的 ID。服务会另存 recorded_at_ms，不能将客户端填的
known_at_ms 当成经过服务器证明的首次发现时间。
GET 不抓取 source_url，只保存来源；页面显示文本而不执行其内容。

## 事件窗口模型

workspace `run` 的 model 为 `announcement-window/v1`：

```json
{
  "announcement":"替换为上面完整 announcement 对象",
  "instrument_id":"venue-a-btc","as_of_ms":40000,
  "horizon_ms":10000,"max_gap_ms":1000,
  "samples":[
    {"time_ms":10000,"known_at_ms":10000,"price":100,"evidence":"fixture-before"},
    {"time_ms":30020,"known_at_ms":30020,"price":101,"evidence":"fixture-after"}
  ]
}
```

样例字段有占位，需替换 announcement 对象。锚点用 known_at_ms，不是发布时间；
从当时已知的样本选基准价，在锚点+horizon 后 max_gap 范围内选终点。
缺基准/终点返回 insufficient_data；窗口还没发生、时间乱序、未来数据直接拒绝。
输出 return_bps 是描述性价格变化，非事件因果效应、净利润或可成交回测。

拿它研究“上币公告后波动/反应速度”可以；用它证明看见公告就能以公告前价格买入不可以。
异常收益、基准市场调整和多事件统计显著性需要独立研究，不能靠一次事件窗口下结论。
