# 00 · 从启动到第一次可复现实验

MarketBridge 是数据与研究服务，不是自动交易机器人。没有交易账户、私钥、下单端点。
支持范围由具体连接器和模型决定，不承诺免费接口覆盖全部市场、无延迟或不被限流。

## 1. 启动当前源码

在项目根目录运行以下 PowerShell 命令。需要 Rust stable 与 Windows C++ 编译工具。
旧 `v0.0.5` 发布包不包含本轮未发布功能，不能拿旧包验证本手册。

```powershell
cargo +stable build --locked
$env:MARKETBRIDGE_CONFIG = 'config.research.yaml'
$env:MARKETBRIDGE_API_KEY = [Guid]::NewGuid().ToString('N')
$env:MARKETBRIDGE_RESEARCH_DB = 'data/research-workspace.sqlite'
.\target\debug\market-bridge.exe
```

默认监听 `127.0.0.1:8080`。零采集器模式不联网，也不会自动产生行情。
同一 PowerShell 会话可以先保存 API Key；不要把真实 Key 放进代码、截图或 URL。
想边运行边输入其他命令，开第二个终端并使用相同 Key。

Linux/macOS 使用相同配置，变量写作 `export MARKETBRIDGE_CONFIG=config.research.yaml`，
程序路径为 `./target/debug/market-bridge`。

## 2. 使用内置工作台

打开 `http://127.0.0.1:8080/workbench`。这是程序自带的静态页面，无需 Node.js、
另外的前端服务器或数据库管理界面。填入 Key → 连接并检查。

进入“研究实验”，选择 `same-asset-spot/v1` → 装入合成场景示例 → 运行并归档。
这是固定的测试订单簿，不是真实市场行情。样例预期存在负成本后差额及深度不足的档位，
不是为了展示盈利而挑选的数据。

进入“归档与公告”，命名空间 `runs`，点击查询；实验输入、结果或失败原因都保留。
实验 ID 不能覆盖。第二次运行先换 ID，或者加载样例自动生成新 ID。

## 3. 直接调用 API 与 CLI

```powershell
$headers = @{'x-api-key' = $env:MARKETBRIDGE_API_KEY}
Invoke-RestMethod 'http://127.0.0.1:8080/v1/system/info' -Headers $headers
$body = Get-Content 'examples/research/same-asset.json' -Raw
Invoke-RestMethod 'http://127.0.0.1:8080/v1/research/evaluate' `
  -Method Post -Headers $headers -ContentType application/json -Body $body
.\target\debug\market-bridge.exe --evaluate examples/research/same-asset.json
```

CLI 直接计算，不启动服务，也不自动归档；使用工作区 `run` 操作才保存实验。
`/health` 只表示 HTTP 服务能响应；检查行情还要看来源、更新时间、订单簿完整性与错误。

## 4. 切换到公共行情观察

先 Ctrl+C 停服务，再将 `MARKETBRIDGE_CONFIG` 改成 `config.research-live.yaml` 启动。
该配置观察 Binance / OKX 的 BTC 现货；不代表平台只能研究 BTC。
更多市场按[数据源配置](03-sources.md)接入，成本和资产身份仍需明确提供。

若要清空试验，从新数据库路径开始；不要直接删除历史数据库或覆盖旧实验。
正常退出用 Ctrl+C。发生强制退出后先执行工作区 `integrity` 检查。
