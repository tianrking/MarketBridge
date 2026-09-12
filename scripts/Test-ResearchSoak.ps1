param(
    [ValidateRange(10,259200)][int]$DurationSeconds=60,
    [ValidateRange(1,60)][int]$IntervalSeconds=5,
    [switch]$PublicSources,
    [string]$Binary=(Join-Path $PSScriptRoot '..\target\debug\market-bridge.exe')
)
# Bounded observation harness, not automatic production certification.
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$binaryPath=(Resolve-Path $Binary).Path
$out=Join-Path $repo 'examples\out'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$runId=[Guid]::NewGuid().ToString('N')
$prefix=Join-Path $out "soak-$runId"
$gitSha=(& git -C $repo rev-parse HEAD)
$worktreeDirty=[bool](& git -C $repo status --porcelain)
$names=@('MARKETBRIDGE_CONFIG','MARKETBRIDGE_API_ADDR','MARKETBRIDGE_API_KEY','MARKETBRIDGE_RESEARCH_DB','MARKETBRIDGE_RECORD_DIR','MARKETBRIDGE_CONTROL_FILE')
$saved=@{};foreach($name in $names){$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
$probe=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
$probe.Start();$port=$probe.LocalEndpoint.Port;$probe.Stop()
$process=$null;$writer=$null;$samples=0;$errors=0;$both=0;$maxMemory=0L;$previousIds='';$changes=0
try {
    $env:MARKETBRIDGE_CONFIG=Join-Path $repo $(if($PublicSources){'config.research-live.yaml'}else{'config.research.yaml'})
    $env:MARKETBRIDGE_API_ADDR="127.0.0.1:$port"
    $env:MARKETBRIDGE_API_KEY=[Guid]::NewGuid().ToString('N')
    $env:MARKETBRIDGE_RESEARCH_DB="$prefix.sqlite"
    Remove-Item Env:MARKETBRIDGE_RECORD_DIR,Env:MARKETBRIDGE_CONTROL_FILE -ErrorAction SilentlyContinue
    $headers=@{'x-api-key'=$env:MARKETBRIDGE_API_KEY};$base="http://127.0.0.1:$port"
    $process=Start-Process -FilePath $binaryPath -WorkingDirectory $repo -PassThru -WindowStyle Hidden -RedirectStandardOutput "$prefix.stdout.log" -RedirectStandardError "$prefix.stderr.log"
    $writer=[IO.StreamWriter]::new("$prefix.samples.jsonl",$false,[Text.UTF8Encoding]::new($false))
    $ready=$false
    for($i=0;$i -lt 40;$i++){try{$null=Invoke-RestMethod "$base/v1/system/info" -Headers $headers -TimeoutSec 2;$ready=$true;break}catch{Start-Sleep -Milliseconds 250}}
    if(-not $ready){throw 'Observation server failed to start'}
    $route=Get-Content (Join-Path $repo 'examples/research/scan-live.json') -Raw
    $watch=[Diagnostics.Stopwatch]::StartNew()
    while($watch.Elapsed.TotalSeconds -lt $DurationSeconds){
        $process.Refresh();if($process.HasExited){throw 'Observation process exited'}
        $memory=$process.WorkingSet64;$maxMemory=[Math]::Max($maxMemory,$memory)
        $sample=@{sequence=++$samples;utc=[DateTime]::UtcNow.ToString('o');working_set_bytes=$memory;cpu_seconds=$process.TotalProcessorTime.TotalSeconds}
        $latency=[Diagnostics.Stopwatch]::StartNew()
        try {
            $null=Invoke-RestMethod "$base/v1/system/info" -Headers $headers -TimeoutSec 5
            if($PublicSources){
                $scan=Invoke-RestMethod "$base/v1/research/scan-live" -Headers $headers -Method Post -ContentType application/json -Body $route -TimeoutSec 5
                if($scan.ranking.Count -ne 0){throw 'Unverified example became ranked opportunity'}
                $candidate=$scan.candidates[0];$sample.error=$candidate.error
                if($candidate.result){$both++;$ids="$($candidate.result.buy_observation_id)|$($candidate.result.sell_observation_id)";if($ids -ne $previousIds){$changes++;$previousIds=$ids};$sample.buy_observation=$candidate.result.buy_observation_id;$sample.sell_observation=$candidate.result.sell_observation_id;$sample.reasons=$candidate.result.points[0].reasons}
            }
        }catch{$errors++;$sample.error=$_.Exception.Message}
        $sample.http_elapsed_ms=$latency.ElapsedMilliseconds
        $writer.WriteLine(($sample|ConvertTo-Json -Compress -Depth 12));$writer.Flush()
        $bytes=(Get-Item -LiteralPath "$prefix.samples.jsonl").Length+(Get-Item -LiteralPath "$prefix.stdout.log").Length+(Get-Item -LiteralPath "$prefix.stderr.log").Length
        if($bytes -gt 256MB){throw 'Observation logs exceeded 256 MiB; stopping without deleting evidence'}
        Start-Sleep -Milliseconds ([int][Math]::Max(1,[Math]::Min($IntervalSeconds*1000,($DurationSeconds-$watch.Elapsed.TotalSeconds)*1000)))
    }
    $report=@{kind='bounded_observation_not_release_certification';git_sha=$gitSha;worktree_dirty=$worktreeDirty;binary_sha256=(Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash;ended_at_utc=[DateTime]::UtcNow.ToString('o');requested_seconds=$DurationSeconds;elapsed_seconds=$watch.Elapsed.TotalSeconds;public_sources=[bool]$PublicSources;samples=$samples;http_errors=$errors;samples_with_both_books=$both;changed_observation_pairs=$changes;max_working_set_bytes=$maxMemory;orders_placed=$false;log_prefix=$prefix;release_certified=$false}
    $report|ConvertTo-Json -Depth 12|Set-Content -LiteralPath "$prefix.report.json" -Encoding utf8NoBOM
    $report|ConvertTo-Json -Depth 12
    if($errors -gt 0){throw 'Observation completed with HTTP errors; inspect evidence before acceptance'}
}finally{
    if($writer){$writer.Dispose()}
    if($process -and -not $process.HasExited){Stop-Process -Id $process.Id}
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
}
