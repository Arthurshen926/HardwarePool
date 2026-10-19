$ErrorActionPreference='Stop'
# Execute only the parsed parameter block, never a deployment script body.
$names=@('stage_native_route_service_lab.ps1','stage_display_diagnostics_lab.ps1','stage_display_peer_lab.ps1','repair_display_control_account_lab.ps1','update_integrated_capyio_service_lab.ps1')
foreach($name in $names) {
 $tokens=$null;$errors=$null
 $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name),[ref]$tokens,[ref]$errors)
 if($errors.Count){throw "Parse error: $name"}
 $binding=[scriptblock]::Create($ast.ParamBlock.Extent.Text + "`n'bound'")
 $values=@{}
 foreach($parameter in $ast.ParamBlock.Parameters) {
  $key=$parameter.Name.VariablePath.UserPath
  if($key -eq 'Deploy'){continue}
  $values[$key]=switch -Regex ($key) {
   'Sid$' { 'S-1-5-21-1-2-3-1001'; break }
   'Ipv4$' { '192.0.2.10'; break }
   'StageName$' { 'display-fixture'; break }
   default { 'fixture' }
  }
 }
 if((& $binding @values) -ne 'bound'){throw 'Valid target rejected'}
 foreach($bad in @('::1','not-an-address','999.1.1.1')) {
  $values.LocalIpv4=$bad
  $rejected=$false
  try {& $binding @values|Out-Null} catch [Management.Automation.ParameterBindingException] {$rejected=$true}
  if(-not $rejected){throw "Invalid IPv4 accepted by $name"}
 }
 $values.LocalIpv4='192.0.2.10'
 if($values.ContainsKey('ControlUserSid')) {
  $values.ControlUserSid='not-a-sid'
  $rejected=$false
  try {& $binding @values|Out-Null} catch [Management.Automation.ParameterBindingException] {$rejected=$true}
  if(-not $rejected){throw "Invalid SID accepted by $name"}
 }
 Write-Output "$name parameter_contract=pass deployment_body_executed=false"
}
