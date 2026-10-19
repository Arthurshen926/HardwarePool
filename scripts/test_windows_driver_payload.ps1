$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'windows_driver_payload.ps1')
$directory=Join-Path (Split-Path -Parent $PSScriptRoot) ('target\driver-contract-tests\'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $directory 'audio'),(Join-Path $directory 'touchpad') -Force|Out-Null
$packages=@()
foreach($role in @('audio-apo','audio-extension','audio-base','touchpad')){
    $folder=if($role -eq 'touchpad'){'touchpad'}else{'audio'}
    $ids=@{
      'audio-apo'=@('SWC\VEN_CAPY&CID_RENDERAPO','5989fce8-9cd0-467d-8a6a-5419e31529d4')
      'audio-extension'=@('Root\CapyIO_VirtualSpeaker','e2f84ce7-8efa-411c-aa69-97454ca4cb57')
      'audio-base'=@('Root\CapyIO_VirtualSpeaker','4d36e96c-e325-11ce-bfc1-08002be10318')
      'touchpad'=@('Root\CapyIOVhfTouchpad','4d36e97d-e325-11ce-bfc1-08002be10318')
    }
    "[Version]`nCatalogFile=$role.cat`nClassGuid={$($ids[$role][1])}`n[Models]`nDevice=Install,$($ids[$role][0])"|Set-Content (Join-Path $directory "$folder/$role.inf")
    'synthetic-not-a-driver'|Set-Content (Join-Path $directory "$folder/$role.cat")
    $packages+=@(@{role=$role;inf="$folder/$role.inf";catalog="$folder/$role.cat";signerThumbprint=('A'*40)})
}
$files=@(Get-ChildItem $directory -Recurse -File|ForEach-Object {@{path=$_.FullName.Substring($directory.Length+1).Replace('\','/');sha256=(Get-FileHash $_.FullName).Hash}})
$manifest=@{schemaVersion=1;architecture='x64';signingPolicy='microsoft-hardware-dashboard';packages=$packages;files=$files}
$manifest|ConvertTo-Json -Depth 5|Set-Content (Join-Path $directory 'driver-bundle.json')
# Signature checks are explicitly mocked; nothing here is a valid driver input.
$script:signatureStatus='Valid'
function Get-AuthenticodeSignature { @{Status=$script:signatureStatus;SignerCertificate=@{Thumbprint=('A'*40)}} }
Read-CapyDriverBundle $directory|Out-Null
$script:signatureStatus='NotSigned'
try{Read-CapyDriverBundle $directory|Out-Null;throw 'Unsigned input accepted'}catch{if($_.Exception.Message -notlike 'Catalog signature invalid*'){throw}}
$script:signatureStatus='Valid'
'changed'|Add-Content (Join-Path $directory 'audio/audio-base.inf')
try{Read-CapyDriverBundle $directory|Out-Null;throw 'Tampered input accepted'}catch{if($_.Exception.Message -notlike 'Driver payload hash mismatch:*'){throw}}
Add-Type -Path (Join-Path $PSScriptRoot 'windows_driver_setup.cs')
'PASS: manifest/signature rejection/hash mismatch; native interop compilation only, no installation API calls.'
