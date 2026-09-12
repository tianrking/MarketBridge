# 06 · 库存、部分成交、组合与退出

两个模型都只做预充值现货场景模拟，不连接真实账户，不自动借钱，没有保证成交。

## 固定双边模型

`prefunded-taker-scenario/v1`：`initial` 指定两个场所的 base/quote 余额，frames 指定
evidence、size_index、buy_fill_fraction、sell_fill_fraction。适合一对固定标的。
详见[原有 paper API](../research-api.md)。

## 多标的分配库存模型

工作区 run 的 model 使用 `allocated-spot-portfolio/v1`，input 包括：

```json
{
  "accounts": [
    {"instrument": "替换为完整买方 Instrument 对象", "base": 0, "quote": 100000},
    {"instrument": "替换为完整卖方 Instrument 对象", "base": 2, "quote": 0}
  ],
  "steps": [{
    "id": "entry-001", "purpose": "entry", "gate": {"kind": "always"},
    "evidence": "替换为完整 ScanRequest 对象",
    "size_index": 0, "buy_fill_fraction": 1, "sell_fill_fraction": 0.5
  }]
}
```

上面是字段说明，不可直接运行的占位 JSON；工作台选择该模型并装入示例会生成可执行对象。
账户 2–128 个，步骤 1–512 个，步骤 ID 唯一、时间不倒退；每个 venue/symbol 只允许一个
分配账户，报价币种与价格单位必须相同。只支持乘数为 1 的现货；不同资产身份不能复用同一 base ID。

同场所不同交易对的 quote 是**明确分配的子预算**，不是同一笔钱复制多份。
例如总资金 1000，两个子账户只能按 600+400 等方式分配，不能各填 1000。
新观察 ID 会补充新的场景深度，但不能改同 ID 的内容来重复使用已消耗流动性。

## 条件与退出

- `purpose`: entry / exit / rebalance，用于标注操作目的。
- `gate`: `{"kind":"always"}`，`{"kind":"net_bps_at_least","bps":2}`，或
  `{"kind":"time_at_or_after","time_ms":...}`。
- `exit` 还要求这步不增加相对初始库存的绝对净 base 敞口。

买入只成交、卖出未成交时，可以在后续步骤反向卖回已有 base；需要交换 buy/sell 的
Instrument、真实反向盘口与 relationship 方向，并给出新的成本。不要用旧买入价假定能止损。
时间到了也不保证退出：缺行情、深度不足、余额不足、成本未知仍会拒绝或保持未成交。

## 读懂结果

entries 返回每步是否条件满足、是否仅参考、深度/库存是否不足及具体模拟数量。
accounts 是最终分配余额；residual_base_changes 是按资产汇总的净库存变化。
net_cash_change_quote 只是现金变化。仅所有净 base 敞口回到初始值时，才给出
closed_base_cash_pnl_quote；没有给开放仓位按最新价美化收益。

模型使用浮点研究计算，不是结算账本；不含 margin、强平、借贷、跨链结算、自动 FX、
maker 队列、盘口反事实冲击和跨请求连续持仓。需要这些模型时，应新增独立版本和验收，
不能修改现货公式后继续沿用旧模型名称。
