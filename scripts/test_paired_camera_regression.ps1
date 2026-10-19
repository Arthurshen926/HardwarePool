[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$ConsumerProbe,
    [Parameter(Mandatory)][guid]$PeerNodeId,
    [Parameter(Mandatory)][string]$PeerHost,
    [Parameter(Mandatory)][ValidateRange(1,65535)][int]$ControlPort,
    [Parameter(Mandatory)][string]$EvidenceRoot,
    [ValidateRange(15,300)][int]$ObserveSeconds=30
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'node_api_lab_client.ps1')
function Api($command,$arguments=@{}) {
    $reply=Invoke-NodeCommand $command $arguments
    if($reply.PSObject.Properties['Err']){throw $reply.Err}
    return $reply.Ok
}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'This acceptance check requires an ordinary user process.'}
if(Get-Process capyio-desktop -ErrorAction SilentlyContinue){throw 'Exit the existing node before this isolated acceptance check.'}
$exe=(Resolve-Path -LiteralPath $Executable).Path
$probe=(Resolve-Path -LiteralPath $ConsumerProbe).Path
$evidence=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $evidence){throw 'Use a new evidence directory; earlier failures must remain intact.'}
New-Item -ItemType Directory -Path $evidence|Out-Null
$node=Start-Process -FilePath $exe -ArgumentList '--node-host' -WindowStyle Hidden -PassThru
$cap=$null
$result=[ordered]@{passed=$false;ordinaryUser=$true;startedAtUtc=[DateTime]::UtcNow.ToString('o');desktopSha256=(Get-FileHash $exe).Hash;consumerSha256=(Get-FileHash $probe).Hash;rawMediaSaved=$false}
try {
    if((Api 'host_status').pid -ne $node.Id){throw 'Unexpected Host owner.'}
    Api 'connect_paired_peer_at' @{request=@{nodeId=$PeerNodeId.ToString();host=$PeerHost;port=$ControlPort}}|Out-Null
    $cameras=@((Api 'get_snapshot').capabilities|Where-Object {$_.nodeId -eq $PeerNodeId.ToString() -and $_.capabilityClass -eq 'camera' -and -not $_.simulated})
    if($cameras.Count -ne 1){throw 'Expected one real camera in the authenticated peer catalog.'}
    $cap=$cameras[0].id
    Api 'set_remote_capability_use' @{request=@{capabilityId=$cap;active=$true}}|Out-Null
    Write-Output 'Allow the camera request on the paired device and keep CapyIO in the foreground.'
    $deadline=[DateTime]::UtcNow.AddSeconds(120)
    do {
        $route=@((Api 'get_snapshot').routes|Where-Object {$_.sourceCapabilityId -eq $cap -and $_.state -eq 'active'})
        if($route.Count -eq 1){break}
        Start-Sleep -Milliseconds 500
    }while([DateTime]::UtcNow -lt $deadline)
    if($route.Count -ne 1){throw 'Camera did not become active within the consent/start deadline.'}
    $routeId=$route[0].id
    $frames=@()
    $deadline=[DateTime]::UtcNow.AddSeconds($ObserveSeconds)
    do {
        $current=@((Api 'get_snapshot').routes|Where-Object {$_.id -eq $routeId -and $_.state -eq 'active'})
        if($current.Count -ne 1){throw 'Camera route stopped during observation.'}
        $media=@((Api 'get_diagnostics').checks|Where-Object label -eq 'Route 最近媒体观测（非实时速率）'|ForEach-Object {$_.detail|ConvertFrom-Json}|Where-Object routeId -eq $routeId)
        if($media.Count -ne 1){throw 'Missing unambiguous camera media evidence.'}
        $frames+= $media[0].readiness.localFrames.observed
        $media[0]|ConvertTo-Json -Depth 8 -Compress|Add-Content (Join-Path $evidence 'media.jsonl') -Encoding UTF8
        Start-Sleep -Seconds 2
    }while([DateTime]::UtcNow -lt $deadline)
    if($frames.Count -lt 2 -or $frames[-1] -le $frames[0]){throw 'Published frame count did not increase.'}
    $consumer=Start-Process -FilePath $probe -ArgumentList 'consumer-current-live-probe' -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $evidence 'consumer.log') -RedirectStandardError (Join-Path $evidence 'consumer.err')
    if(-not $consumer.WaitForExit(30000)){$consumer.Kill();$consumer.WaitForExit();throw 'System camera consumer timed out.'}
    if($consumer.ExitCode -ne 0){throw "System camera consumer failed: $($consumer.ExitCode)"}
    $result.firstFrames=$frames[0];$result.lastFrames=$frames[-1];$result.systemConsumerPassed=$true
    Api 'set_remote_capability_use' @{request=@{capabilityId=$cap;active=$false}}|Out-Null
    $stopped=Api 'get_snapshot'
    if(@($stopped.routes|Where-Object {$_.id -eq $routeId -and $_.state -in @('active','starting','stopping')}).Count -or @($stopped.portLeases|Where-Object routeId -eq $routeId).Count){throw 'Camera route or lease remains after Stop.'}
    $result.passed=$true
}catch{
    $result.failure=$_.Exception.Message
    throw
}finally{
    if($cap){
        try{Api 'set_remote_capability_use' @{request=@{capabilityId=$cap;active=$false}}|Out-Null}
        catch{$result.stopFailure=$_.Exception.Message;Write-Warning $_}
    }
    try {
        Api 'host_shutdown'|Out-Null
        if(-not $node.WaitForExit(15000)){throw 'Host cleanup timed out.'}
        $result.hostExited=$true
    }catch{$result.passed=$false;$result.cleanupFailure=$_.Exception.Message;Write-Error $_ -ErrorAction Continue}
    $result|ConvertTo-Json -Depth 5|Set-Content (Join-Path $evidence 'result.json') -Encoding UTF8
}
if(-not $result.passed){exit 1}
