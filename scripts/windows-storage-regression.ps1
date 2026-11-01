param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$DataRoot,[Parameter(Mandatory=$true)][string]$ExpectedCode)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
$DataRoot=(Resolve-Path $DataRoot).Path
if(!(Test-Path (Join-Path $DataRoot '.nyako-acceptance'))){throw 'Missing acceptance marker'}
$db=Join-Path $DataRoot 'data/main.sqlite3'
$before=(Get-FileHash $db -Algorithm SHA256).Hash
$env:NYAKO_ACCEPTANCE_ROOT=$DataRoot
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
Remove-Item Env:NYAKO_ACCEPTANCE_MEASURE -ErrorAction SilentlyContinue
$p=Start-Process $Executable -PassThru
try{
 $condition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty,$p.Id)
 $watch=[Diagnostics.Stopwatch]::StartNew();$found=$false
 do{
  if($p.HasExited){throw 'App exited before storage error appeared'}
  $window=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,$condition)
  if($window){
   $names=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|ForEach-Object {$_.Current.Name})
   if(($names -join ' ') -match [regex]::Escape($ExpectedCode)){$found=$true;break}
  }
  Start-Sleep -Milliseconds 100
 }while($watch.Elapsed.TotalSeconds -lt 30)
 if(!$found){throw "Expected storage code missing: $ExpectedCode"}
 [void]$p.CloseMainWindow()
 if(!$p.WaitForExit(10000)){throw 'Fault window failed to close'}
 $after=(Get-FileHash $db -Algorithm SHA256).Hash
 if($before -ne $after){throw 'Rejected database was modified'}
 @{code=$ExpectedCode;preserved=$true;sha256=$after}|ConvertTo-Json
}finally{if(!$p.HasExited){[void]$p.CloseMainWindow()}}
