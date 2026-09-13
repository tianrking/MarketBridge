param([string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\market-bridge.exe'), [string]$Python = '')
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
$oldResearchDb = $env:MARKETBRIDGE_RESEARCH_DB
$oldControlFile = $env:MARKETBRIDGE_CONTROL_FILE
$process = $null
try {
    $env:MARKETBRIDGE_CONFIG = Join-Path $repo 'config.research.yaml'
    $env:MARKETBRIDGE_API_ADDR = "127.0.0.1:$port"
    $env:MARKETBRIDGE_API_KEY = [Guid]::NewGuid().ToString('N')
    Remove-Item Env:MARKETBRIDGE_RECORD_DIR -ErrorAction SilentlyContinue
    $headers = @{ 'x-api-key' = $env:MARKETBRIDGE_API_KEY }
    $base = "http://127.0.0.1:$port"
    $runId = [Guid]::NewGuid().ToString('N')
    $env:MARKETBRIDGE_RESEARCH_DB = Join-Path $out "workspace-$runId.sqlite"
    $env:MARKETBRIDGE_CONTROL_FILE=Join-Path $out "control-$runId.json"
    @{revision='file-initial';enabled=$false;interval_ms=1000;min_net_bps=0;min_hold_ms=0;record_scans=$false;routes=@()} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $env:MARKETBRIDGE_CONTROL_FILE -Encoding utf8NoBOM
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
    $quotas = Invoke-RestMethod "$base/v1/system/provider-quotas" -Headers $headers
    if ($quotas.domain -ne 'provider_quota_status' -or $null -eq $quotas.quotas -or $quotas.quotas.Count -ne 0) {
        throw 'Default research config did not expose an empty provider quota status'
    }
    $integration = Invoke-RestMethod "$base/v1/integration/context?symbols=BTCUSDT&include_storage=true" -Headers $headers
    if ($integration.domain -ne 'integration_context' -or $null -eq $integration.capabilities) {
        throw 'Integration context contract unavailable'
    }
    $unauthorized = Invoke-WebRequest "$base/v1/system/info" -SkipHttpErrorCheck
    if ($unauthorized.StatusCode -ne 401) { throw 'Expected unauthorized request to fail' }
    $body = Get-Content (Join-Path $repo 'examples\research\same-asset.json') -Raw
    $result = Invoke-RestMethod "$base/v1/research/evaluate" -Headers $headers -Method Post -ContentType application/json -Body $body
    if ($result.points.Count -ne 4 -or $result.points[0].conditional_net_quote -ge 0) {
        throw 'Incorrect conditional cost curve'
    }
    if ($null -ne $result.points[3].conditional_net_quote) { throw 'Insufficient depth produced a net estimate' }
    $inputObject = $body | ConvertFrom-Json
    function Workspace($action) {
        Invoke-RestMethod "$base/v1/research/workspace" -Headers $headers -Method Post -ContentType application/json -Body ($action | ConvertTo-Json -Depth 50)
    }
    $registry = @{id='registry-one'; known_at_ms=9000; evidence='synthetic test'; instruments=@($inputObject.buy.instrument,$inputObject.sell.instrument); relationships=@($inputObject.relationship)}
    $saved=Workspace @{action='registry_put';request=$registry}
    if ($saved.id -ne 'registry-one') { throw 'Registry persistence failed' }
    $pair=Workspace @{action='registry_pair';request=@{revision_id='registry-one';left=$inputObject.buy.instrument.id;right=$inputObject.sell.instrument.id;as_of_ms=10020}}
    if ($pair.buy.id -ne $inputObject.buy.instrument.id) { throw 'Registry relationship resolution failed' }
    $null=Workspace @{action='dataset_append';request=@{dataset_id='fixture';chunk_id='one';frames=@($inputObject)}}
    $archived=Workspace @{action='dataset_replay';request=@{dataset_id='fixture';run_id='dataset-run';after_sequence=0;limit_chunks=1}}
    if ($archived.payload.chunks.Count -ne 1) { throw 'Archived dataset replay failed' }
    $failedRun=Workspace @{action='run';request=@{id='failed-run';model='not-a-model';input=@{}}}
    if ($failedRun.payload.output.status -ne 'failed') { throw 'Failed experiment not retained' }
    $checked=Workspace @{action='integrity'}
    if ($checked.sqlite -ne 'ok') { throw 'Research store integrity failed' }
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
    $control=@{revision='http-one';enabled=$false;interval_ms=1000;min_net_bps=0;min_hold_ms=0;record_scans=$false;routes=@()}
    $fileControl=@{revision='file-updated';enabled=$false;interval_ms=2000;min_net_bps=0;min_hold_ms=0;record_scans=$false;routes=@()}
    $fileControl | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $env:MARKETBRIDGE_CONTROL_FILE -Encoding utf8NoBOM
    $loaded=$false
    for ($i=0;$i -lt 15;$i++) {Start-Sleep -Milliseconds 300;$fileStatus=Invoke-RestMethod "$base/v1/research/control" -Headers $headers;if($fileStatus.config.revision -eq 'file-updated'){$loaded=$true;break}}
    if(-not $loaded){throw 'Scanner file hot reload did not apply'}
    '{invalid' | Set-Content -LiteralPath $env:MARKETBRIDGE_CONTROL_FILE -Encoding utf8NoBOM
    $rejected=$false
    for ($i=0;$i -lt 15;$i++) {Start-Sleep -Milliseconds 300;$fileStatus=Invoke-RestMethod "$base/v1/research/control" -Headers $headers;if($fileStatus.reload_error){$rejected=$true;break}}
    if(-not $rejected -or $fileStatus.config.revision -ne 'file-updated'){throw 'Invalid file did not retain previous config'}
    $null=Invoke-RestMethod "$base/v1/research/control" -Headers $headers -Method Post -ContentType application/json -Body ($control|ConvertTo-Json -Depth 20)
    $control.interval_ms=0
    $invalid=Invoke-WebRequest "$base/v1/research/control" -Headers $headers -Method Post -ContentType application/json -Body ($control|ConvertTo-Json -Depth 20) -SkipHttpErrorCheck
    if ($invalid.StatusCode -ne 422) {throw 'Invalid control accepted'}
    $status=Invoke-RestMethod "$base/v1/research/control" -Headers $headers
    if ($status.config.interval_ms -ne 1000) {throw 'Invalid control replaced last good config'}
    $event=@{id='fixture-event';source='fixture';source_url='https://example.test/event';title='Synthetic event';category='listing';instrument_ids=@();known_at_ms=10000}
    $null=Workspace @{action='announcement_put';request=$event}
    $again=Workspace @{action='announcement_put';request=$event}
    if ($again.id -ne 'fixture-event') {throw 'Announcement idempotence failed'}
    $workbench=Invoke-WebRequest "$base/workbench"
    if ($workbench.Content -notmatch '研究工作台' -or -not $workbench.Headers['Content-Security-Policy']) {throw 'Workbench assets/CSP unavailable'}
    Stop-Process -Id $process.Id
    $process.WaitForExit()
    $process=Start-Process -FilePath $binaryPath -WorkingDirectory $repo -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $out "api-$runId.restart.stdout.log") `
        -RedirectStandardError (Join-Path $out "api-$runId.restart.stderr.log")
    $recovered=$false
    for ($i=0;$i -lt 40;$i++) {try {$status=Invoke-RestMethod "$base/v1/research/control" -Headers $headers -TimeoutSec 2;$recovered=$true;break} catch {Start-Sleep -Milliseconds 250}}
    if (-not $recovered -or $status.config.revision -ne 'http-one') {throw 'Persisted control did not recover after process restart'}
    $restored=Workspace @{action='get';request=@{namespace='runs';id='dataset-run'}}
    if ($restored.document.id -ne 'dataset-run') {throw 'Archived run lost after restart'}
    if ($Python) {
        $modelOutput = & $Python (Join-Path $repo 'scripts/Test-ResearchModels.py') --base-url $base 2>&1
        $modelOutput | Write-Output
        if ($LASTEXITCODE -ne 0) {throw ("Archived model integration suite failed: " + ($modelOutput | Out-String))}
        $asyncOutput = & $Python (Join-Path $repo 'scripts/Test-AsyncResearch.py') --base-url $base 2>&1
        $asyncOutput | Write-Output
        if ($LASTEXITCODE -ne 0) {throw ("Async HTTP integration suite failed: " + ($asyncOutput | Out-String))}
    }
    Write-Output 'PASS: auth, integration/quota status, cost/replay/paper/batch, CLI parity, registry/datasets/archives, scanner validation, events, workbench assets, forced-process restart recovery'
} finally {
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id }
    $env:MARKETBRIDGE_CONFIG = $oldConfig
    $env:MARKETBRIDGE_API_ADDR = $oldAddr
    $env:MARKETBRIDGE_API_KEY = $oldKey
    $env:MARKETBRIDGE_RECORD_DIR = $oldRecording
    $env:MARKETBRIDGE_RESEARCH_DB = $oldResearchDb
    $env:MARKETBRIDGE_CONTROL_FILE = $oldControlFile
}
