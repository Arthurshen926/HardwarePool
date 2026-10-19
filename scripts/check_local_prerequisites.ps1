[CmdletBinding()]
param()
# Read-only inventory. Presence is not proof that a projection works.
$ErrorActionPreference = 'Stop'
$checks = [Collections.Generic.List[object]]::new()
function Add-Check($Name, $State, $Detail) {
    $checks.Add([pscustomobject]@{ Component=$Name; State=$State; Detail=$Detail })
}
try {
    $service = Get-Service -Name CapyIOBroker -ErrorAction Stop
    Add-Check 'CapyIO Broker' ([string]$service.Status) 'Existing service only; this check does not start or reconfigure it.'
} catch {
    Add-Check 'CapyIO Broker' 'Not confirmed' $_.Exception.Message
}
try {
    $devices = @(Get-CimInstance Win32_PnPEntity -ErrorAction Stop)
    foreach ($entry in @(
        @('Speaker projection', 'CapyIO.*Speaker'),
        @('Microphone projection', 'CapyIO.*Microphone'),
        @('Touchpad projection', 'CapyIO.*(Touchpad|Precision)'),
        @('Display projection', 'SudoMaker Virtual Display Adapter'),
        @('Xbox projection', 'ViGEm|Virtual Gamepad Emulation Bus')
    )) {
        $matches = @($devices | Where-Object { $_.Name -match $entry[1] })
        if ($matches.Count) {
            $details = ($matches | ForEach-Object { "$($_.Name) (device code $($_.ConfigManagerErrorCode))" }) -join '; '
            # Presence and Windows device health are separate observations.
            $faults = @($matches | Where-Object { $null -ne $_.ConfigManagerErrorCode -and $_.ConfigManagerErrorCode -ne 0 })
            $unknown = @($matches | Where-Object { $null -eq $_.ConfigManagerErrorCode })
            $state = if ($faults.Count) { 'Device error' } elseif ($unknown.Count) { 'Health unknown' } else { 'Detected' }
            Add-Check $entry[0] $state $details
        } else {
            Add-Check $entry[0] 'Not detected' 'Name-based inventory only; check installation before using this feature.'
        }
    }
} catch {
    Add-Check 'Windows device inventory' 'Unavailable' $_.Exception.Message
}
Add-Check 'Camera / DS4' 'Application check required' 'Open the corresponding CapyIO resource; PnP inventory alone does not verify virtual-camera registration or VIIPER.'
Add-Check 'Pairing' 'Not checked' 'No credentials were read or changed. Use the Windows account that originally paired.'
$checks | Format-Table -Wrap -AutoSize
Write-Output 'Read-only report: no drivers, services, APKs, permissions or settings were installed or changed.'
Write-Output 'Not detected/Unavailable is not proof of a missing component. Detected is not hardware acceptance.'
