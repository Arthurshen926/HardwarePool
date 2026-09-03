param(
    [ValidateRange(10, 600)]
    [int]$TimeoutSeconds = 300,

    [switch]$CloseAfterFirstScroll
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$form = New-Object System.Windows.Forms.Form
$form.Text = 'CapyIO Touchpad Scroll Target'
$form.StartPosition = 'CenterScreen'
$form.ClientSize = New-Object System.Drawing.Size(640, 360)
$form.TopMost = $true
$form.KeyPreview = $true

$target = New-Object System.Windows.Forms.Label
$target.Dock = 'Fill'
$target.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
$target.Font = New-Object System.Drawing.Font('Segoe UI', 22)
$target.Text = "Waiting for a Windows two-finger scroll..."
$form.Controls.Add($target)

$script:wheelEvents = 0
$script:wheelDelta = 0
$form.Add_MouseWheel({
    $script:wheelEvents += 1
    $script:wheelDelta += $_.Delta
    $target.Text = "Wheel events: $script:wheelEvents`r`nDelta: $script:wheelDelta"
    if ($CloseAfterFirstScroll) {
        $form.Close()
    }
})
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

Write-Output 'scroll_target_status=ready'
Write-Output "scroll_target_timeout_seconds=$TimeoutSeconds"
[System.Windows.Forms.Application]::Run($form)
Write-Output 'scroll_target_status=closed'
Write-Output "scroll_target_events=$script:wheelEvents"
Write-Output "scroll_target_delta=$script:wheelDelta"
