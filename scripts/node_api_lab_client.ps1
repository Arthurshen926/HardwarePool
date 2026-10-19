# Named-pipe client for explicitly invoked local-lab checks. No UI automation.
function Invoke-NodeCommand([string]$Command, $Arguments = @{}, [int]$Version = 1) {
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
    $pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', "CapyIO.Node.v1.$sid.$session", [System.IO.Pipes.PipeDirection]::InOut, [System.IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(5000)
        $payload = [Text.Encoding]::UTF8.GetBytes((@{version=$Version;command=$Command;args=$Arguments} | ConvertTo-Json -Compress -Depth 10))
        $writer = [System.IO.BinaryWriter]::new($pipe, [Text.Encoding]::UTF8, $true)
        $writer.Write([int]$payload.Length)
        $writer.Write($payload)
        $writer.Flush()
        $header = [byte[]]::new(4)
        Read-NodeBytes $pipe $header
        $length = [BitConverter]::ToInt32($header,0)
        if ($length -le 0 -or $length -gt 1048576) { throw 'Node response outside bounds' }
        $reply = [byte[]]::new($length)
        Read-NodeBytes $pipe $reply
        $pipe.WriteByte(1)
        $pipe.Flush()
        return [Text.Encoding]::UTF8.GetString($reply) | ConvertFrom-Json
    } finally { $pipe.Dispose() }
}
function Read-NodeBytes($Pipe, [byte[]]$Buffer) {
    $offset = 0
    while ($offset -lt $Buffer.Length) {
        $read = $Pipe.ReadAsync($Buffer, $offset, $Buffer.Length - $offset)
        if (-not $read.Wait(10000)) { throw 'Node response timeout' }
        if ($read.Result -eq 0) { throw 'Node response closed' }
        $offset += $read.Result
    }
}
