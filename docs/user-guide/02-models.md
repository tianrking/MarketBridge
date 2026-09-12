# 各类研究模型与单位约定

模型通过工作区 `run` 调用；结果及错误均留档。

| model | 含义 | 结果边界 |
|---|---|---|
| `same-asset-spot/v1` | 双边盘口同数量成本曲线 | 条件估算，不保证双腿成交 |
| `candidate-screen/v1` | 候选路径批量筛选 | 排名不是组合资金分配 |
| `scenario-replay/v1` | 给定场景时间顺序回放 | 不是自动生成历史数据 |
| `prefunded-taker-scenario/v1` | 双边库存与显式成交比例模拟 | 不含借贷、保证金和自动退出 |
| `allocated-spot-portfolio/v1` | 分配子账户、多标的步骤与反向退出 | 预充值现货；[组合说明](06-portfolio.md) |
| `spot-derivative-basis/v1` | 同底层资产现货—衍生品价差 | 参考值，不是无风险套利利润 |
| `unit-premium/v1` | 相同报价单位下的包装/发行方溢折价 | 不承诺转换、赎回或收敛 |
| `funding-rate-comparison/v1` | 统一周期后的资金费率差 | 不预测未来实际资金费 |
| `announcement-window/v1` | 公告可知时刻前后的价格窗口 | 描述性关联；[事件说明](07-events.md) |

全部模型的可运行合成输入可离线生成：

```powershell
python scripts/Test-ResearchModels.py --export examples/out/model-inputs-001
```

目标目录必须不存在，避免覆盖研究输入。生成后可导入工作台相应模型运行。
不带 `--export` 时脚本会对运行中的本地 API 依次运行、归档并检查全部模型；它不是市场回测。

## 期现基差与溢折价输入

`input` 字段包含 `model, as_of_ms, max_age_ms, max_skew_ms, left, right,
relationship, carry_cost_per_unit`。`left/right` 各包含：

- `instrument`：完整 Instrument；
- `price`：正的有限参考价；
- `known_at_ms`、`source_time_ms`：知识时间与源时间；
- `evidence`：来源说明。

本工具统一采用 **右腿价格减左腿价格** 的符号，bps 的分母是左腿价格。
期现模型左腿须为现货，右腿为 Future/Perp，底层资产与报价身份一致，关系须
为 `hedge`。`price_unit` 必须一致，例如两边都为 USD/oz，不能把每手价格和每盎司
价格直接比较。`contract_multiplier` 用于披露每份衍生品代表的底层数量。

到期期货还输出按剩余天数、365 天简单年化的参考基差。临近到期可出现很大的
年化数字，这不是预期年收益率。永续合约不输出到期年化。

`carry_cost_per_unit` 是调用者指定的每底层单位成本；未知保留 null。减去它后
仍只是参考差额，没有交易深度、借贷资格、保证金、企业行为或赎回资格保证。

不同市场的“basis”符号可能不同，因此本工具不以名称猜测符号。
相关定义参考 [CME FX 基差说明](https://www.cmegroup.com/education/courses/introduction-to-fx/importance-of-fx-futures-pricing-and-basis)。

## 资金费率比较输入

`input` 包含 `as_of_ms, max_age_ms, long, short`。每腿包含
`instrument, rate, interval_ms, known_at_ms, evidence`。

`rate` 用小数：0.0001 代表 0.01%，不是 0.0001%。采用正费率时多头支付空头的
约定；各来源接入时须先确认符号。周期不能写死为八小时。

小时费率差 = 空腿费率 × 3600000 / 空腿周期毫秒 − 多腿费率 × 3600000 / 多腿周期毫秒。
例如 8 小时 0.0008 与 1 小时 0.0001 归一后相同，不能直接相减制造优势。

该值不是未来收益预测，亦不自动适用于反向/quanto 合约。平仓费、借贷、保证金、
清算和变化中的未来费率未被自动计入。来源约定参考
[Bybit 合约规则](https://www.bybit.com/en/contract-rules/)。
