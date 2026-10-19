$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'manage_windows_installation.ps1'),[ref]$tokens,[ref]$errors)
if($errors){throw ($errors|Out-String)}
foreach($name in @('RestoreSnapshot','AssertPayload')){
    $definition=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
    . ([scriptblock]::Create($definition.Extent.Text))
}
# Only the extracted restore function runs; every system mutation below is a fake.
$script:operations=[Collections.Generic.List[string]]::new()
$cameraKey='fake-camera-key';$serviceName='CapyIOBroker'
function StopBroker {$script:operations.Add('stop')}
function StartBroker {$script:operations.Add('start')}
function Sc($Arguments){$script:operations.Add(($Arguments -join '|'))}
function Get-Service {return @{Status='Stopped'}}
function Test-Path {return $true}
function New-Item {}
function Set-Item {param($LiteralPath,$Value) $script:operations.Add('camera='+$Value)}
function Set-ItemProperty {}
function Remove-ItemProperty {}
function Remove-Item {$script:operations.Add('remove-camera')}
RestoreSnapshot @{service=@{exists=$true;imagePath='old-image';running=$true};camera=@{exists=$true;path='old-camera';threading='Both'}}
if(($script:operations -join ',') -ne 'stop,config|CapyIOBroker|binPath=|old-image,start,camera=old-camera'){throw 'Existing installation rollback failed'}
$script:operations.Clear()
RestoreSnapshot @{service=@{exists=$false};camera=@{exists=$false}}
if(($script:operations -join ',') -ne 'stop,delete|CapyIOBroker,remove-camera'){throw 'Fresh installation rollback failed'}
# Reject package traversal before attempting to read a payload.
try {AssertPayload @{schemaVersion=1;kind='capyio-unified-local-windows';productionRelease=$false;buildId='alpha-20261004';artifacts=@(@{file='../escape';sha256='00'})} '.';throw 'Traversal accepted'}
catch {if($_.Exception.Message -ne 'Invalid payload path.'){throw}}
'PASS: existing-state restore, fresh-state restore, invalid package path rejection; no system mutation.'
