param(
    [ValidateRange(10, 600)]
    [int]$TimeoutSeconds = 30,

    [switch]$CloseAfterCompletedDrag
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$form = New-Object System.Windows.Forms.Form
$form.Text = 'CapyIO Touchpad Drag Target'
$form.StartPosition = 'CenterScreen'
$form.ClientSize = New-Object System.Drawing.Size(900, 520)
$form.TopMost = $true
$form.KeyPreview = $true

$target = New-Object System.Windows.Forms.Label
$target.Dock = 'Fill'
$target.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
$target.Font = New-Object System.Drawing.Font('Segoe UI', 22)
$target.Text = "Waiting for one fixed Windows tap-and-drag..."
$form.Controls.Add($target)

$script:downCount = 0
$script:upCount = 0
$script:moveWithLeftCount = 0
$script:doubleClickCount = 0
$script:dragOrigin = [System.Drawing.Point]::Empty
$script:maxDeltaX = 0
$script:maxDeltaY = 0

$form.Add_Shown({
    $form.Activate()
    $form.BringToFront()
    $form.Focus()
})
$target.Add_MouseDown({
    if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
        $script:downCount += 1
        $script:dragOrigin = [System.Windows.Forms.Cursor]::Position
        $target.Text = "Left downs: $script:downCount`r`nMoves while held: $script:moveWithLeftCount"
    }
})
$target.Add_MouseMove({
    if (([System.Windows.Forms.Control]::MouseButtons -band [System.Windows.Forms.MouseButtons]::Left) -ne 0) {
        $script:moveWithLeftCount += 1
        $position = [System.Windows.Forms.Cursor]::Position
        $script:maxDeltaX = [Math]::Max(
            $script:maxDeltaX,
            [Math]::Abs($position.X - $script:dragOrigin.X)
        )
        $script:maxDeltaY = [Math]::Max(
            $script:maxDeltaY,
            [Math]::Abs($position.Y - $script:dragOrigin.Y)
        )
        $target.Text = "Left downs: $script:downCount`r`nMoves while held: $script:moveWithLeftCount"
    }
})
$target.Add_MouseUp({
    if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
        $script:upCount += 1
        if (
            $CloseAfterCompletedDrag -and
            $script:downCount -ge 1 -and
            $script:upCount -ge 1 -and
            $script:moveWithLeftCount -gt 0 -and
            ($script:maxDeltaX -ge 20 -or $script:maxDeltaY -ge 20)
        ) {
            $form.Close()
        }
    }
})
$target.Add_MouseDoubleClick({
    if ($_.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
        $script:doubleClickCount += 1
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

Write-Output 'drag_target_status=ready'
Write-Output "drag_target_timeout_seconds=$TimeoutSeconds"
[System.Windows.Forms.Application]::Run($form)
Write-Output 'drag_target_status=closed'
Write-Output "drag_target_downs=$script:downCount"
Write-Output "drag_target_ups=$script:upCount"
Write-Output "drag_target_moves_with_left=$script:moveWithLeftCount"
Write-Output "drag_target_double_clicks=$script:doubleClickCount"
Write-Output "drag_target_max_delta=$script:maxDeltaX,$script:maxDeltaY"
Write-Output "drag_target_completed=$($script:downCount -ge 1 -and $script:upCount -ge 1 -and $script:moveWithLeftCount -gt 0 -and ($script:maxDeltaX -ge 20 -or $script:maxDeltaY -ge 20))"
