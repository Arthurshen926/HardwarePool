[CmdletBinding()]
param(
    [ValidateSet('Check','Install','Rollback','Remove','Launch')][string]$Mode='Check',
    [string]$PackageRoot=$PSScriptRoot,
    [string]$UserSid='',
    [switch]$Elevated
)
$ErrorActionPreference='Stop'
$root=Join-Path $env:ProgramW6432 'CapyIO'
$statePath=Join-Path $root 'installation.json'
$cameraKey='Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\{35754be3-54b6-4133-a1c7-1716395c6f1c}\InprocServer32'
$serviceName='CapyIOBroker'
function Sc([string[]]$Arguments) {
    & "$env:SystemRoot\System32\sc.exe" @Arguments | Out-Null
    if($LASTEXITCODE -ne 0){throw "Service operation failed: $($Arguments[0]) ($LASTEXITCODE)"}
}
function ServiceSnapshot {
    $s=Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if(-not $s){return @{exists=$false}}
    return @{exists=$true;imagePath=$s.PathName;running=($s.State -eq 'Running');account=$s.StartName;startMode=$s.StartMode}
}
function CameraSnapshot {
    if(-not(Test-Path -LiteralPath $cameraKey)){return @{exists=$false}}
    $key=Get-Item -LiteralPath $cameraKey
    return @{exists=$true;path=$key.GetValue('');threading=$key.GetValue('ThreadingModel')}
}
function StopBroker {
    $s=Get-Service $serviceName -ErrorAction SilentlyContinue
    if($s -and $s.Status -ne 'Stopped'){$s.Stop();$s.WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30))}
}
function StartBroker {
    Start-Service $serviceName
    (Get-Service $serviceName).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))
}
function RestoreSnapshot($before) {
    StopBroker
    if($before.service.exists){
        Sc @('config',$serviceName,'binPath=',$before.service.imagePath)
        if($before.service.running){StartBroker}
    }elseif(Get-Service $serviceName -ErrorAction SilentlyContinue){Sc @('delete',$serviceName)}
    if($before.camera.exists){
        New-Item -Path $cameraKey -Force|Out-Null
        Set-Item -LiteralPath $cameraKey -Value $before.camera.path
        if($null -ne $before.camera.threading){Set-ItemProperty -LiteralPath $cameraKey -Name ThreadingModel -Value $before.camera.threading}
        else{Remove-ItemProperty -LiteralPath $cameraKey -Name ThreadingModel -ErrorAction SilentlyContinue}
    }elseif(Test-Path -LiteralPath $cameraKey){Remove-Item -LiteralPath $cameraKey -Recurse}
}
function AssertProtectedRoot {
    if(Test-Path -LiteralPath $root){
        if((Get-Item -LiteralPath $root).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Installation root must not be a link.'}
        $acl=Get-Acl -LiteralPath $root
        foreach($rule in $acl.Access){
            $sid=$rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
            if($rule.AccessControlType -eq 'Allow' -and $sid -notin @('S-1-5-18','S-1-5-32-544','S-1-3-0','S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464') -and ($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::Write)){
                throw 'Installation directory grants non-administrator write access.'
            }
        }
    }
}
function ReadState {
    if(Test-Path -LiteralPath $statePath){
        if((Get-Item -LiteralPath $statePath).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Installation journal must not be a link.'}
        return Get-Content -LiteralPath $statePath -Raw|ConvertFrom-Json
    }
    return $null
}
function SaveState($state){
    $state|ConvertTo-Json -Depth 12|Set-Content -LiteralPath "$statePath.new" -Encoding UTF8
    Move-Item -LiteralPath "$statePath.new" -Destination $statePath -Force
}
function DriverChecks {
    $devices=@(Get-CimInstance Win32_PnPEntity)
    foreach($item in @(@('audio','^CapyIO (Virtual Audio|Speaker with Render Bridge)$'),@('touchpad','^CapyIO VHF Precision Touchpad Source$'),@('audio-apo','^CapyIO Render APO$'))){
        $found=@($devices|Where-Object {$_.Name -match $item[1] -and $_.ConfigManagerErrorCode -eq 0})
        [pscustomobject]@{component=$item[0];ready=($found.Count -gt 0);detail='Existing healthy device required; no driver/security-policy changes.'}
    }
}
function AssertPayload($manifest,$package){
    if($manifest.schemaVersion -ne 1 -or $manifest.kind -ne 'capyio-unified-local-windows' -or $manifest.productionRelease){throw 'Unsupported package.'}
    if($manifest.buildId -notmatch '^alpha-[0-9-]+$'){throw 'Invalid build identity.'}
    foreach($entry in $manifest.artifacts){
        if($entry.file -notmatch '^(?:[A-Za-z0-9_.-]+|drivers/driver-bundle\.json|drivers/(?:audio|touchpad)/[A-Za-z0-9_.-]+\.(?:inf|cat|sys|dll))$' -or $entry.file -in @('.','..')){throw 'Invalid payload path.'}
        $file=Get-Item -LiteralPath (Join-Path $package $entry.file)
        if($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or (Get-FileHash -LiteralPath $file.FullName).Hash -ne $entry.sha256){throw "Payload mismatch: $($entry.file)"}
    }
    foreach($name in @('capyio-desktop.exe','capyio-windows-service.exe','capyio-native-virtual-speaker.exe','capyio-native-virtual-microphone.exe','capyio_windows_camera_mf.dll','manage_windows_installation.ps1','windows_driver_payload.ps1','windows_driver_setup.cs')){
        if($name -notin $manifest.artifacts.file){throw "Missing payload: $name"}
    }
}
function AssertActive($state){
    if(-not $state){throw 'No managed installation.'}
    $service=ServiceSnapshot;$camera=CameraSnapshot
    if(-not $service.exists -or $service.imagePath -cne $state.imagePath -or -not $camera.exists -or $camera.path -cne $state.cameraPath){throw 'System configuration changed outside this installer; inspect before rollback/removal.'}
}
try {
    AssertProtectedRoot
    $state=ReadState
    if($Mode -eq 'Launch'){
        if(-not $state){throw 'CapyIO is not installed.'}
        if($state.userSid -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value){throw 'Use the account that installed CapyIO.'}
        $application=Join-Path $state.activeVersion 'capyio-desktop.exe'
        if(-not $application.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid installed application path.'}
        Start-Process -FilePath $application
        exit 0
    }
    if($Mode -eq 'Check'){
        $drivers=@(DriverChecks)
        @{mode='Check';installation=$state;service=ServiceSnapshot;camera=CameraSnapshot;drivers=$drivers;corePrerequisitesReady=(@($drivers|Where-Object {-not $_.ready}).Count -eq 0);driverDeployment='optional-signed-payload; see package manifest';publicationApproved=$false}|ConvertTo-Json -Depth 12
        exit 0
    }
    if(-not $Elevated){
        $UserSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $script=(Resolve-Path -LiteralPath $PSCommandPath).Path
        $package=(Resolve-Path -LiteralPath $PackageRoot).Path
        if($script -match '"' -or $package -match '"'){throw 'Unsupported path.'}
        $argsLine='-NoProfile -ExecutionPolicy Bypass -File "'+$script+'" -Mode '+$Mode+' -PackageRoot "'+$package+'" -UserSid '+$UserSid+' -Elevated'
        $process=Start-Process "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Verb RunAs -WindowStyle Hidden -ArgumentList $argsLine -PassThru -Wait
        exit $process.ExitCode
    }
    if(-not [Environment]::Is64BitProcess){throw '64-bit Windows PowerShell required.'}
    $principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator confirmation required.'}
    if($UserSid -notmatch '^S-1-5-21-(\d+-){3}\d+$'){throw 'An explicit original-user SID is required.'}
    if(Get-Process capyio-desktop -ErrorAction SilentlyContinue){throw 'Exit the CapyIO node before changing versions (closing its window is insufficient).'}
    if($state -and $state.userSid -ne $UserSid){throw 'Use the Windows account that owns this installation.'}
    if($state){AssertActive $state}
    if($Mode -ne 'Install'){
        $before=@{service=ServiceSnapshot;camera=CameraSnapshot}
        $history=@($state.history)
        if(-not $history.Count){throw 'No previous state recorded.'}
        $restore=if($Mode -eq 'Remove'){$history[0]}else{$history[-1]}
        try {RestoreSnapshot $restore}catch{RestoreSnapshot $before;throw}
        if($Mode -eq 'Rollback' -and $restore.activeVersion){
            $state.activeVersion=$restore.activeVersion;$state.imagePath=$restore.service.imagePath;$state.cameraPath=$restore.camera.path
            $state.history=@($history|Select-Object -First ($history.Count-1));SaveState $state
        }else{
            # Retain payloads and receipts for recovery; no pairing or user files are deleted.
            Move-Item -LiteralPath $statePath -Destination (Join-Path $root ('removed-'+[DateTime]::UtcNow.ToString('yyyyMMddHHmmss')+'.json'))
        }
        Write-Output "$Mode completed; retained version files and user data."
        exit 0
    }
    $package=(Resolve-Path -LiteralPath $PackageRoot).Path
    $manifest=Get-Content -LiteralPath (Join-Path $package 'windows-installation.json') -Raw|ConvertFrom-Json
    AssertPayload $manifest $package
    $missing=@(DriverChecks|Where-Object {-not $_.ready})
    if($missing.Count -gt 0 -and $missing.Count -lt 3){throw 'Partial driver installation requires explicit repair; new-machine automation does not modify existing components.'}
    $hasDrivers=$manifest.PSObject.Properties['signedDriverPayloadIncluded'] -and $manifest.signedDriverPayloadIncluded
    if($missing.Count -and -not $hasDrivers){throw ('Core driver prerequisites missing/unhealthy: '+($missing.component -join ', ')+'. No changes made. Supply the approved signed driver bundle when building setup.')}
    if($hasDrivers){
        . (Join-Path $package 'windows_driver_payload.ps1')
        Read-CapyDriverBundle (Join-Path $package 'drivers')|Out-Null
    }
    $oldService=ServiceSnapshot
    if($oldService.exists -and ($oldService.account -ne 'LocalSystem' -or $oldService.imagePath -notmatch '^"[^"\r\n]*\\capyio-windows-service\.exe"')){throw 'Unrecognized existing Broker configuration.'}
    $before=@{service=$oldService;camera=CameraSnapshot;activeVersion=$(if($state){$state.activeVersion}else{$null})}
    $destination=Join-Path $root $manifest.buildId
    if(Test-Path -LiteralPath $destination){
        if((Get-Item -LiteralPath $destination).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Version directory must not be a link.'}
        if($state -and $state.activeVersion -eq $destination){throw 'This version is already active.'}
        # Re-enter an intact payload after a driver-required reboot; never overwrite it.
        AssertPayload $manifest $destination
    }else{
        New-Item -ItemType Directory -Path $destination -Force|Out-Null
        foreach($entry in $manifest.artifacts){
            $target=Join-Path $destination $entry.file
            New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force|Out-Null
            Copy-Item -LiteralPath (Join-Path $package $entry.file) -Destination $target
        }
        Copy-Item -LiteralPath (Join-Path $package 'windows-installation.json') -Destination $destination
    }
    AssertPayload $manifest $destination
    AssertProtectedRoot
    if($missing.Count){
        . (Join-Path $destination 'windows_driver_payload.ps1')
        $driverResult=Install-CapyNewMachineDrivers (Join-Path $destination 'drivers') $destination { @(DriverChecks|Where-Object {-not $_.ready}).Count -eq 0 }
        if($driverResult.rebootRequired){Write-Output 'Drivers require restart. Restart manually, then run this installer again.';exit 3010}
    }
    $exe=Join-Path $destination 'capyio-windows-service.exe'
    $speaker=Join-Path $destination 'capyio-native-virtual-speaker.exe'
    $microphone=Join-Path $destination 'capyio-native-virtual-microphone.exe'
    if($oldService.exists){
        $image=[regex]::Replace($oldService.imagePath,'^"[^"]+"','"'+$exe+'"')
        foreach($item in @(@('broker',$speaker),@('microphone-broker',$microphone))){
            $pattern='"?--'+$item[0]+'"?\s+"[^"]+"'
            if($image -notmatch $pattern){throw "Existing configuration has no $($item[0]) path."}
            $image=[regex]::Replace($image,$pattern,'"--'+$item[0]+'" "'+$item[1]+'"')
        }
        foreach($option in @('audio-route-control-user-sid','camera-ingress-user-sid')){
            $pattern='"?--'+$option+'"?\s+"?S-1-[0-9-]+"?'
            $image=[regex]::Replace($image,$pattern,'')
            $image+=' "--'+$option+'" "'+$UserSid+'"'
        }
    }else{
        $image='"'+$exe+'" --mode native-speaker --broker "'+$speaker+'" --bind-ip 127.0.0.1 --port 40101 --peer-ip 127.0.0.2 --peer-port 40100 --microphone-broker "'+$microphone+'" --microphone-bind-ip 127.0.0.1 --microphone-port 40111 --microphone-peer-ip 127.0.0.2 --microphone-peer-port 40110 --audio-route-control-user-sid '+$UserSid+' --camera-ingress-user-sid '+$UserSid
    }
    $camera=Join-Path $destination 'capyio_windows_camera_mf.dll'
    $before|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $destination 'before-install.json') -Encoding UTF8
    try {
        StopBroker
        New-Item -Path $cameraKey -Force|Out-Null
        Set-Item -LiteralPath $cameraKey -Value $camera
        Set-ItemProperty -LiteralPath $cameraKey -Name ThreadingModel -Value 'Both'
        if($oldService.exists){Sc @('config',$serviceName,'binPath=',$image)}
        else{Sc @('create',$serviceName,'binPath=',$image,'start=','auto','obj=','LocalSystem')}
        StartBroker
        $new=@{schemaVersion=1;activeVersion=$destination;imagePath=$image;cameraPath=$camera;userSid=$UserSid;history=@();retainedVersions=@();atUtc=[DateTime]::UtcNow.ToString('o')}
        if($state){$new.history=@($state.history);$new.retainedVersions=@($state.retainedVersions)}
        $new.history+=@($before);$new.retainedVersions+=@($destination)
        SaveState $new
    }catch{
        $failure=$_.Exception.Message
        try{RestoreSnapshot $before}catch{$failure+='; rollback failed: '+$_.Exception.Message}
        throw $failure
    }
    Write-Output ('Installed: '+$destination)
}catch{
    $message=$_.Exception.Message
    Write-Error $message -ErrorAction Continue
    if($Elevated){
        Add-Type -AssemblyName System.Windows.Forms
        [Windows.Forms.MessageBox]::Show($message,'CapyIO 安装未完成','OK','Error')|Out-Null
    }
    exit 1
}
