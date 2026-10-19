# Dot-source only. These definitions do not install or remove anything.
function Read-CapyDriverBundle([string]$Directory) {
    $root=(Resolve-Path -LiteralPath $Directory).Path
    $manifest=Get-Content -LiteralPath (Join-Path $root 'driver-bundle.json') -Raw|ConvertFrom-Json
    if($manifest.schemaVersion -ne 1 -or $manifest.architecture -ne 'x64' -or $manifest.signingPolicy -ne 'microsoft-hardware-dashboard'){throw 'Unsupported driver bundle contract.'}
    $roles=@('audio-apo','audio-extension','audio-base','touchpad')
    if(@($manifest.packages).Count -ne 4 -or (@($manifest.packages.role|Sort-Object) -join ',') -ne (($roles|Sort-Object) -join ',')){throw 'Expected exactly the four reviewed driver roles.'}
    $seen=@{}
    foreach($file in $manifest.files){
        if($file.path -notmatch '^(audio|touchpad)/[A-Za-z0-9_.-]+\.(inf|cat|sys|dll)$' -or $seen.ContainsKey($file.path) -or $file.sha256 -notmatch '^[A-Fa-f0-9]{64}$'){throw 'Invalid driver file entry.'}
        foreach($part in @($root,(Join-Path $root ($file.path.Split('/')[0])),(Join-Path $root $file.path))){
            if((Get-Item -LiteralPath $part).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Driver payload cannot contain links.'}
        }
        if((Get-FileHash -LiteralPath (Join-Path $root $file.path)).Hash -ne $file.sha256){throw "Driver payload hash mismatch: $($file.path)"}
        $seen[$file.path]=$file.sha256
    }
    foreach($package in $manifest.packages){
        $prefix=if($package.role -eq 'touchpad'){'touchpad/'}else{'audio/'}
        if(-not $package.inf.StartsWith($prefix) -or -not $package.inf.EndsWith('.inf') -or -not $package.catalog.StartsWith($prefix) -or -not $package.catalog.EndsWith('.cat') -or -not $seen.ContainsKey($package.inf) -or -not $seen.ContainsKey($package.catalog)){throw 'INF and catalog must be inventoried in their component directory.'}
        if($package.signerThumbprint -notmatch '^[A-Fa-f0-9]{40}$'){throw 'Pin the verified catalog signer thumbprint.'}
        $signature=Get-AuthenticodeSignature -LiteralPath (Join-Path $root $package.catalog)
        if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $package.signerThumbprint){throw "Catalog signature invalid or differs from approved signer: $($package.role)"}
        # Catalog trust is an early check; Windows still enforces catalog membership and kernel policy.
        $inf=Get-Content -LiteralPath (Join-Path $root $package.inf) -Raw
        $identities=@{
            'audio-base'=@('Root\CapyIO_VirtualSpeaker','4d36e96c-e325-11ce-bfc1-08002be10318')
            'audio-extension'=@('Root\CapyIO_VirtualSpeaker','e2f84ce7-8efa-411c-aa69-97454ca4cb57')
            'audio-apo'=@('SWC\VEN_CAPY&CID_RENDERAPO','5989fce8-9cd0-467d-8a6a-5419e31529d4')
            'touchpad'=@('Root\CapyIOVhfTouchpad','4d36e97d-e325-11ce-bfc1-08002be10318')
        }
        $identity=$identities[$package.role]
        if($inf -notmatch ('(?im)^\s*ClassGuid\s*=\s*\{'+$identity[1]+'\}\s*$') -or $inf -notmatch ('(?im),\s*'+[regex]::Escape($identity[0])+'\s*$')){throw 'INF class or hardware identity differs from the reviewed role.'}
        $catalogName=[IO.Path]::GetFileName($package.catalog)
        if($inf -notmatch ('(?im)^\s*CatalogFile(?:\.NTamd64)?\s*=\s*'+[regex]::Escape($catalogName)+'\s*$')){throw 'INF catalog does not match its manifest.'}
    }
    return $manifest
}
function Initialize-CapyDriverApi {
    if(-not ('CapyDriverSetup' -as [type])){Add-Type -Path (Join-Path $PSScriptRoot 'windows_driver_setup.cs')}
}
function Install-CapyNewMachineDrivers([string]$Directory,[string]$JournalDirectory,[scriptblock]$HealthCheck) {
    $bundle=Read-CapyDriverBundle $Directory
    $principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if(-not [Environment]::Is64BitProcess -or -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw '64-bit administrator process required.'}
    if([int](Get-CimInstance Win32_OperatingSystem).BuildNumber -lt 22000){throw 'Windows 11 or later required by the VHF package contract.'}
    Initialize-CapyDriverApi
    $roots=@(@{role='audio-base';hardware='Root\CapyIO_VirtualSpeaker'},@{role='touchpad';hardware='Root\CapyIOVhfTouchpad'})
    foreach($root in $roots){if([CapyDriverSetup]::Existing($root.hardware).Length){throw 'Partial/existing driver installation requires explicit repair; automatic root creation is new-machine only.'}}
    $journal=Join-Path $JournalDirectory ('driver-install-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-ffff')+'.json')
    $result=@{status='installing';rebootRequired=$false;staged=@();created=@();retainedDriverStorePackages=$true;bundleSha256=(Get-FileHash (Join-Path $Directory 'driver-bundle.json')).Hash;host=$env:COMPUTERNAME}
    function SaveDriverJournal {$result|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $journal -Encoding UTF8}
    SaveDriverJournal
    try {
        foreach($role in @('audio-apo','audio-extension','audio-base','touchpad')){
            $package=@($bundle.packages|Where-Object role -eq $role)[0]
            $result.rebootRequired=[CapyDriverSetup]::Stage((Join-Path $Directory $package.inf)) -or $result.rebootRequired
            $result.staged+=@($package.inf);SaveDriverJournal
        }
        foreach($root in $roots){
            $package=@($bundle.packages|Where-Object role -eq $root.role)[0]
            $inf=Join-Path $Directory $package.inf
            $instance=[CapyDriverSetup]::CreateRoot($inf,$root.hardware)
            $result.created+=@(@{instance=$instance;hardware=$root.hardware});SaveDriverJournal
            $result.rebootRequired=[CapyDriverSetup]::Bind($inf,$root.hardware) -or $result.rebootRequired
            SaveDriverJournal
        }
        if(-not $result.rebootRequired){
            $deadline=[DateTime]::UtcNow.AddSeconds(30)
            while(-not (& $HealthCheck)){
                if([DateTime]::UtcNow -ge $deadline){throw 'Installed devices did not reach healthy state.'}
                Start-Sleep -Milliseconds 500
            }
        }
        $result.status=if($result.rebootRequired){'reboot-required'}else{'installed-and-healthy'}
        SaveDriverJournal
        return [pscustomobject]$result
    }catch{
        $result.status='failed';$result.failure=$_.Exception.Message;$result.cleanupErrors=@()
        foreach($device in @($result.created|Sort-Object instance -Descending)){
            try{[CapyDriverSetup]::RemoveCreated($device.instance)}catch{$result.cleanupErrors+=@($_.Exception.Message)}
        }
        SaveDriverJournal
        throw ('Driver installation failed; journal: '+$journal+'; '+$result.failure)
    }
}
