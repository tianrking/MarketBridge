param([string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\market-bridge.exe'))
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$binaryPath = (Resolve-Path $Binary).Path
$out = Join-Path $repo 'examples\out'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$probe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$probe.Start()
$port = $probe.LocalEndpoint.Port
$probe.Stop()
$oldConfig = $env:MARKETBRIDGE_CONFIG
$oldAddr = $env:MARKETBRIDGE_API_ADDR
$oldKey = $env:MARKETBRIDGE_API_KEY
$oldRecording = $env:MARKETBRIDGE_RECORD_DIR
$process = $null
try {
    $env:MARKETBRIDGE_CONFIG = Join-Path $repo 'config.research.yaml'
    $env:MARKETBRIDGE_API_ADDR = "127.0.0.1:$port"
    $env:MARKETBRIDGE_API_KEY = [Guid]::NewGuid().ToString('N')
    Remove-Item Env:MARKETBRIDGE_RECORD_DIR -ErrorAction SilentlyContinue
    $headers = @{ 'x-api-key' = $env:MARKETBRIDGE_API_KEY }
    $base = "http://127.0.0.1:$port"
    $runId = [Guid]::NewGuid().ToString('N')
    $process = Start-Process -FilePath $binaryPath -WorkingDirectory $repo -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $out "api-$runId.stdout.log") `
        -RedirectStandardError (Join-Path $out "api-$runId.stderr.log")
    $ready = $false
    for ($i = 0; $i -lt 40; $i++) {
        if ($process.HasExited) { throw "Server exited with $($process.ExitCode)" }
        try {
            $null = Invoke-RestMethod "$base/v1/system/info" -Headers $headers -TimeoutSec 2
            $ready = $true
            break
        } catch { Start-Sleep -Milliseconds 250 }
    }
    if (-not $ready) { throw 'Server did not become ready' }
    $unauthorized = Invoke-WebRequest "$base/v1/system/info" -SkipHttpErrorCheck
    if ($unauthorized.StatusCode -ne 401) { throw 'Expected unauthorized request to fail' }
    $body = Get-Content (Join-Path $repo 'examples\research\same-asset.json') -Raw
    $result = Invoke-RestMethod "$base/v1/research/evaluate" -Headers $headers -Method Post -ContentType application/json -Body $body
    if ($result.points.Count -ne 4 -or $result.points[0].conditional_net_quote -ge 0) {
        throw 'Incorrect conditional cost curve'
    }
    if ($null -ne $result.points[3].conditional_net_quote) { throw 'Insufficient depth produced a net estimate' }
    $inputObject = $body | ConvertFrom-Json
    $inputObject.costs.buy_fee_bps = $null
    $missing = Invoke-RestMethod "$base/v1/research/evaluate" -Headers $headers -Method Post -ContentType application/json -Body ($inputObject | ConvertTo-Json -Depth 30)
    if ($null -ne $missing.points[0].conditional_net_quote) { throw 'Unknown fee became zero' }
    $inputObject = $body | ConvertFrom-Json
    $replayBody = @{frames=@($inputObject)} | ConvertTo-Json -Depth 30
    $replay = Invoke-RestMethod "$base/v1/research/replay" -Headers $headers -Method Post -ContentType application/json -Body $replayBody
    if ($replay.decisions.Count -ne 1) { throw 'Replay failed' }
    $paperInput = @{
        initial = @{ buy_venue_quote=1000000; buy_venue_base=0; sell_venue_quote=0; sell_venue_base=10 }
        frames = @(@{ evidence=$inputObject; size_index=0; buy_fill_fraction=1; sell_fill_fraction=1 })
    }
    $paper = Invoke-RestMethod "$base/v1/research/paper" -Headers $headers -Method Post -ContentType application/json -Body ($paperInput | ConvertTo-Json -Depth 30)
    if ($paper.residual_base_change -ne 0 -or [Math]::Abs($paper.closed_base_cash_pnl_quote - $result.points[0].conditional_net_quote) -gt 0.0000001) { throw 'Paired paper ledger disagrees with cost curve' }
    $paperInput.frames[0].sell_fill_fraction = 0
    $partial = Invoke-RestMethod "$base/v1/research/paper" -Headers $headers -Method Post -ContentType application/json -Body ($paperInput | ConvertTo-Json -Depth 30)
    if ($partial.residual_base_change -ne 0.5 -or $null -ne $partial.closed_base_cash_pnl_quote) { throw 'Unmatched leg incorrectly reported closed PnL' }
    $positive = $body | ConvertFrom-Json
    $positive.costs.buy_fee_bps = 0
    $positive.costs.sell_fee_bps = 0
    $batchInput = @{as_of_ms=$positive.as_of_ms; min_net_bps=0; candidates=@(
        @{id='positive-fixture'; evidence=$positive}, @{id='negative-fixture'; evidence=($body | ConvertFrom-Json)}
    )}
    $batch = Invoke-RestMethod "$base/v1/research/scan" -Headers $headers -Method Post -ContentType application/json -Body ($batchInput | ConvertTo-Json -Depth 30)
    if ($batch.candidates.Count -ne 2 -or $batch.ranking.Count -ne 3 -or $batch.ranking[0].candidate_id -ne 'positive-fixture') { throw 'Incorrect batch screening' }
    # Generated, uniquely named test inputs stay with the ignored test outputs.
    $batchPath=Join-Path $out "batch-$runId.json"
    $paperPath=Join-Path $out "paper-$runId.json"
    $batchInput | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $batchPath -Encoding utf8NoBOM
    $paperInput | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $paperPath -Encoding utf8NoBOM
    $cliBatch = & $binaryPath --scan $batchPath | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $cliBatch.ranking.Count -ne $batch.ranking.Count -or $cliBatch.ranking[0].conditional_net_quote -ne $batch.ranking[0].conditional_net_quote) { throw 'CLI/API batch disagreement' }
    $cliPaper = & $binaryPath --paper $paperPath | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $cliPaper.residual_base_change -ne $partial.residual_base_change -or $null -ne $cliPaper.closed_base_cash_pnl_quote) { throw 'CLI/API paper disagreement' }
    $route = @{buy=$positive.buy.instrument; sell=$positive.sell.instrument; relationship=$positive.relationship; quantities=$positive.quantities; costs=$positive.costs; max_age_ms=1000; max_skew_ms=100}
    $live = Invoke-RestMethod "$base/v1/research/scan-live" -Headers $headers -Method Post -ContentType application/json -Body (@{min_net_bps=0; candidates=@(@{id='absent'; route=$route})} | ConvertTo-Json -Depth 30)
    if ($live.ranking.Count -ne 0 -or $null -eq $live.candidates[0].error) { throw 'Missing cached books became a live opportunity' }
    $inputObject.sell.received_at_ms = 20000
    $futureBody = @{frames=@($inputObject)} | ConvertTo-Json -Depth 30
    $future = Invoke-WebRequest "$base/v1/research/replay" -Headers $headers -Method Post -ContentType application/json -Body $futureBody -SkipHttpErrorCheck
    if ($future.StatusCode -ne 422) { throw 'Future information was accepted' }
    $inputObject = $body | ConvertFrom-Json
    $inputObject.buy.bids[0].price = 99999
    $crossed = Invoke-WebRequest "$base/v1/research/evaluate" -Headers $headers -Method Post -ContentType application/json -Body ($inputObject | ConvertTo-Json -Depth 30) -SkipHttpErrorCheck
    if ($crossed.StatusCode -ne 422) { throw 'Crossed book was accepted' }
    Write-Output 'PASS: HTTP auth, costs/capacity, replay/paper/batch, CLI parity, missing live books, future/crossed-book rejection'
} finally {
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id }
    $env:MARKETBRIDGE_CONFIG = $oldConfig
    $env:MARKETBRIDGE_API_ADDR = $oldAddr
    $env:MARKETBRIDGE_API_KEY = $oldKey
    $env:MARKETBRIDGE_RECORD_DIR = $oldRecording
}
