param(
    [ValidateRange(10, 600)]
    [int]$TimeoutSeconds = 300
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$form = New-Object System.Windows.Forms.Form
$form.Text = 'CapyIO Touchpad Shell Gesture Target'
$form.StartPosition = 'CenterScreen'
$form.ClientSize = New-Object System.Drawing.Size(640, 360)
$form.TopMost = $true
$form.KeyPreview = $true

$target = New-Object System.Windows.Forms.Label
$target.Dock = 'Fill'
$target.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
$target.Font = New-Object System.Drawing.Font('Segoe UI', 22)
$target.Text = "Waiting for a Windows three/four-finger Shell action..."
$form.Controls.Add($target)

$script:activated = 0
$script:deactivated = 0
$form.Add_Activated({ $script:activated += 1 })
$form.Add_Deactivate({ $script:deactivated += 1 })
$form.Add_Shown({
    $form.Activate()
    $form.BringToFront()
    $form.Focus()
})
$form.Add_KeyDown({
    if ($_.KeyCode -eq [System.Windows.Forms.Keys]::Escape) {
        $form.Close()
    }
})

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = $TimeoutSeconds * 1000
$timer.Add_Tick({
    $timer.Stop()
    $form.Close()
})
$timer.Start()

Write-Output 'gesture_target_status=ready'
Write-Output "gesture_target_timeout_seconds=$TimeoutSeconds"
[System.Windows.Forms.Application]::Run($form)
Write-Output 'gesture_target_status=closed'
Write-Output "gesture_target_activated=$script:activated"
Write-Output "gesture_target_deactivated=$script:deactivated"
