param(
    [ValidateRange(10, 600)]
    [int]$TimeoutSeconds = 300,

    [switch]$CloseAfterFirstClick
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$form = New-Object System.Windows.Forms.Form
$form.Text = 'CapyIO Touchpad Click Target'
$form.StartPosition = 'CenterScreen'
$form.ClientSize = New-Object System.Drawing.Size(640, 360)
$form.TopMost = $true
$form.KeyPreview = $true

$target = New-Object System.Windows.Forms.Button
$target.Dock = 'Fill'
$target.Font = New-Object System.Drawing.Font('Segoe UI', 22)
$target.Text = "Tap once on the phone touchpad`r`nWaiting for a Windows click..."
$form.Controls.Add($target)
$form.Add_Shown({
    $form.Activate()
    $form.BringToFront()
})

$script:clickCount = 0
$target.Add_Click({
    $script:clickCount += 1
    $target.Text = "Clicks received: $script:clickCount`r`nTap again, or press Esc to close"
    Write-Output "click_observed=$script:clickCount"
    if ($CloseAfterFirstClick) {
        $form.Close()
    }
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

Write-Output "click_target_status=ready"
Write-Output "click_target_timeout_seconds=$TimeoutSeconds"
[System.Windows.Forms.Application]::Run($form)
Write-Output "click_target_status=closed"
if ($script:clickCount -gt 0) {
    Write-Output "click_observed=$script:clickCount"
}
Write-Output "click_target_total=$script:clickCount"
