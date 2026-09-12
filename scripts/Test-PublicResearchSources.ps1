param(
    [ValidateRange(10, 60)][int]$ObservationSeconds = 20,
    [string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\market-bridge.exe')
)
# Opt-in bounded network observation, not a deterministic CI test or profit test.
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
    $env:MARKETBRIDGE_CONFIG = Join-Path $repo 'config.research-live.yaml'
    $env:MARKETBRIDGE_API_ADDR = "127.0.0.1:$port"
    $env:MARKETBRIDGE_API_KEY = [Guid]::NewGuid().ToString('N')
    Remove-Item Env:MARKETBRIDGE_RECORD_DIR -ErrorAction SilentlyContinue
    $headers = @{ 'x-api-key' = $env:MARKETBRIDGE_API_KEY }
    $base = "http://127.0.0.1:$port"
    $runId = [Guid]::NewGuid().ToString('N')
    $env:MARKETBRIDGE_RESEARCH_DB=Join-Path $out "public-$runId.sqlite"
    Remove-Item Env:MARKETBRIDGE_CONTROL_FILE -ErrorAction SilentlyContinue
    $process = Start-Process -FilePath $binaryPath -WorkingDirectory $repo -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $out "public-$runId.stdout.log") `
        -RedirectStandardError (Join-Path $out "public-$runId.stderr.log")
    $ready = $false
    for ($i=0; $i -lt 40; $i++) {
        if ($process.HasExited) { throw "Server exited with $($process.ExitCode); inspect public-$runId logs" }
        try { $null=Invoke-RestMethod "$base/v1/system/info" -Headers $headers -TimeoutSec 2; $ready=$true; break }
        catch { Start-Sleep -Milliseconds 250 }
    }
    if (-not $ready) { throw 'Server did not become ready' }
    $body=Get-Content (Join-Path $repo 'examples\research\scan-live.json') -Raw
    $watch=[System.Diagnostics.Stopwatch]::StartNew()
    $bothBooks=0
    $buyObservations=[System.Collections.Generic.HashSet[string]]::new()
    $sellObservations=[System.Collections.Generic.HashSet[string]]::new()
    $samples=0
    $latestError=$null
    $latestReasons=@()
    while ($watch.Elapsed.TotalSeconds -lt $ObservationSeconds) {
        $scan=Invoke-RestMethod "$base/v1/research/scan-live" -Headers $headers -Method Post -ContentType application/json -Body $body -TimeoutSec 5
        $samples++
        if ($scan.ranking.Count -ne 0) { throw 'Unverified example unexpectedly produced ranked opportunities' }
        $candidate=$scan.candidates[0]
        $latestError=$candidate.error
        if ($null -ne $candidate.result) {
            $bothBooks++
            $null=$buyObservations.Add($candidate.result.buy_observation_id)
            $null=$sellObservations.Add($candidate.result.sell_observation_id)
            $latestReasons=@($candidate.result.points[0].reasons)
        }
        Start-Sleep -Seconds 1
    }
    [pscustomobject]@{
        evidence_level='short_public_network_observation'
        observed_at_utc=[DateTime]::UtcNow.ToString('o')
        requested_seconds=$ObservationSeconds
        actual_seconds=[Math]::Round($watch.Elapsed.TotalSeconds,2)
        samples=$samples
        samples_with_both_cached_books=$bothBooks
        distinct_buy_observations=$buyObservations.Count
        distinct_sell_observations=$sellObservations.Count
        latest_error=$latestError
        latest_reference_reasons=$latestReasons
        log_prefix="public-$runId"
        live_soak_certified=$false
        orders_placed=$false
    } | ConvertTo-Json -Depth 10
} finally {
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id }
    $env:MARKETBRIDGE_CONFIG=$oldConfig
    $env:MARKETBRIDGE_API_ADDR=$oldAddr
    $env:MARKETBRIDGE_API_KEY=$oldKey
    $env:MARKETBRIDGE_RECORD_DIR=$oldRecording
    $env:MARKETBRIDGE_RESEARCH_DB=$oldResearchDb
    $env:MARKETBRIDGE_CONTROL_FILE=$oldControlFile
}
