# 08 · Python SDK 与研究工作台

## SDK 安装

同步客户端只用 Python 标准库；异步客户端使用 HTTPX。建议独立虚拟环境：

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r sdk/python/requirements.txt
$env:PYTHONPATH = (Resolve-Path sdk/python).Path
```

SDK 源文件为 `marketbridge.py` 和 `marketbridge_async.py`，Python 3.10+。
不要在 Git 中提交虚拟环境或真实 Key。也可以将 sdk/python 加入自己的项目模块搜索路径。

```python
import json, os, uuid
from pathlib import Path
from marketbridge import MarketBridge

client = MarketBridge(api_key=os.environ.get("MARKETBRIDGE_API_KEY"))
evidence = json.loads(Path("examples/research/same-asset.json").read_text())
result = client.run("python-" + uuid.uuid4().hex, "same-asset-spot/v1", evidence)
print(result["payload"]["output"])
```

## 真正的异步请求和游标读取

```python
import asyncio, os
from marketbridge_async import AsyncMarketBridge

async def main():
    async with AsyncMarketBridge(api_key=os.environ.get("MARKETBRIDGE_API_KEY")) as client:
        print(await client.get("/v1/system/info"))
        print(await client.get("/v1/market/quotes", params={"symbol": "BTCUSDT"}))
        # 读取当前已有事件后结束；follow=True 则持续轮询直到取消。
        async for document in client.documents("events", after_sequence=0):
            print(document["sequence"], document["payload"])
            # 成功处理后，由应用持久保存 sequence，下次从此游标继续。

asyncio.run(main())
```

同步与异步均提供 evaluate、replay、paper、scan、scan_live、workspace、run、configure。
异步还提供 evaluate_live、get、documents；workspace 可调用全部已实现的登记、数据集、
公告与实验操作，无需 SDK 为每个模型硬编码不同资产公式。

异步连接池默认并发 2，请求总超时默认 15 秒（包含等待并发槽位），请求 2 MiB、响应 8 MiB。
取消会向异步网络请求传播。关闭上下文释放连接。
没有隐式写请求重试、没有跨站重定向，也不自动读取代理环境变量。HTTP 401/422/429、
网络异常和超时向调用方暴露；按服务返回情况决定等待、修正还是停止。

documents 的恢复语义是“处理后保存游标，异常后从最后成功游标重新调用”。消费者需幂等，
不是恰好一次消息系统。这里是持久事件的轮询订阅，不是假称 WebSocket 可回放历史。
原有 `/v1/stream` 等实时 WS 接口见[接口文档](../data_interfaces.md)，其断线缺口需要显式处理。

## 工作台的四个区域

1. 数据与运行状态：查询来源、盘口、报价、资金费率、基差、扫描状态；可开启 5 秒刷新。
   请求失败停止自动刷新，并标注上一次显示结果可能过期。
2. 研究实验：选择模型，编辑/导入 JSON，运行并归档。合成示例只用于现货相关模型；
   合约/公告模型需明确输入，不自动猜测产品和单位。
3. 归档与公告：按 namespace/sequence 查询、导出结果，执行高级 workspace 操作。
4. 自动扫描配置：加载、修改、校验应用或停止扫描。修改已有内容必须新 revision。

Key 只在当前页面输入框内，不写 localStorage/URL。刷新页面需重新输入。
静态工作台页面可公开加载，但所有研究数据与变更请求仍经过 API 鉴权。
“导出”只下载最近一次成功读取的 JSON，不打包数据库或包含 Key。

HTTPX 的连接生命周期和超时行为参考其[异步文档](https://www.python-httpx.org/async/)
与[超时说明](https://www.python-httpx.org/advanced/timeouts/)。
