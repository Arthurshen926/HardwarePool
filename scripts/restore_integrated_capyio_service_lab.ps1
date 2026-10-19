[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$EvidenceRoot,
    [Parameter(Mandatory)][string]$ExpectedCurrentImagePath
)
$ErrorActionPreference='Stop'
$identity=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(-not $identity.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Service restoration requires an approved administrator session.'}
$serviceName='CapyIOBroker'
$evidence=(Resolve-Path -LiteralPath $EvidenceRoot).Path
$prior=Get-Content -LiteralPath (Join-Path $evidence 'original-service.json') -Raw|ConvertFrom-Json
if($prior.name -ne $serviceName){throw 'Backup is not for CapyIOBroker.'}
$current=Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
if(-not $current -or $current.PathName -cne $ExpectedCurrentImagePath){throw 'Service changed after approval; refusing restoration.'}
# Old backups predate the existed field and always described an existing service.
$existed=if($prior.PSObject.Properties['existed']){[bool]$prior.existed}else{$true}
if($existed -and ($prior.startName -cne $current.StartName -or $prior.startMode -cne $current.StartMode)){throw 'Service account or startup mode changed; refusing partial restoration.'}
if($current.State -ne 'Stopped'){
    Stop-Service $serviceName
    (Get-Service $serviceName).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30))
}
if($existed){
    & sc.exe config $serviceName binPath= $prior.imagePath | Out-Null
    if($LASTEXITCODE -ne 0){throw 'Restore service image failed.'}
    if($prior.state -eq 'Running'){
        Start-Service $serviceName
        (Get-Service $serviceName).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))
    }
    $restored=Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if($restored.PathName -cne $prior.imagePath -or $restored.State -cne $prior.state){throw 'Restored service differs from recorded baseline.'}
}else{
    & sc.exe delete $serviceName | Out-Null
    if($LASTEXITCODE -ne 0){throw 'Remove newly installed service failed.'}
    if(Get-CimInstance Win32_Service -Filter "Name='$serviceName'"){throw 'Service removal is still pending.'}
}
@{atUtc=[DateTime]::UtcNow.ToString('o');restored=$true;servicePreviouslyExisted=$existed;preservedUserData=$true}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $evidence 'restore-result.json') -Encoding UTF8
