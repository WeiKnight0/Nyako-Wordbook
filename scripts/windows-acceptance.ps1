param(
 [Parameter(Mandatory=$true)][string]$Executable,
 [Parameter(Mandatory=$true)][string]$DataRoot,
 [ValidateSet('cold','measure','memory')][string]$Scenario='cold',
 [int]$Samples=20,
 [int]$TimeoutSeconds=1800
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$Executable=(Resolve-Path $Executable).Path
$DataRoot=(Resolve-Path $DataRoot).Path
if(!(Test-Path (Join-Path $DataRoot '.nyako-acceptance'))){throw 'Missing acceptance marker'}
if($Samples -lt 1){throw 'Samples must be positive'}
$env:NYAKO_ACCEPTANCE_ROOT=$DataRoot
$env:NYAKO_ACCEPTANCE_TIMING='1'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
if($Scenario -eq 'measure'){$env:NYAKO_ACCEPTANCE_MEASURE='1'}else{Remove-Item Env:NYAKO_ACCEPTANCE_MEASURE -ErrorAction SilentlyContinue}
if($Scenario -eq 'measure'){
 $output=Join-Path $DataRoot 'frontend.jsonl'
 if(Test-Path $output){throw 'frontend.jsonl already exists; use a fresh measurement directory or archive the previous output'}
}
function Measure-ProcessTree([int]$RootProcessId){
 $all=@(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId)
 $ids=[Collections.Generic.HashSet[int]]::new()
 [void]$ids.Add($RootProcessId)
 do{
  $before=$ids.Count
  $all | Where-Object {$ids.Contains([int]$_.ParentProcessId)} | ForEach-Object {[void]$ids.Add([int]$_.ProcessId)}
 }while($ids.Count -gt $before)
 $processes=@($ids | ForEach-Object {Get-Process -Id $_ -ErrorAction SilentlyContinue})
 return @{processes=$processes.Count;working_bytes=($processes|Measure-Object WorkingSet64 -Sum).Sum;private_bytes=($processes|Measure-Object PrivateMemorySize64 -Sum).Sum}
}
$memory=@()
$times=@()
$runs=if($Scenario -eq 'cold'){$Samples}else{1}
for($i=0;$i -lt $runs;$i++){
 $watch=[Diagnostics.Stopwatch]::StartNew()
 $p=Start-Process $Executable -PassThru
 try{
  $condition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty,$p.Id)
  $ready=$false
  while($watch.Elapsed.TotalSeconds -lt 60){
   if($p.HasExited){throw "App exited with $($p.ExitCode)"}
   $window=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,$condition)
   if($window){
    $list=$window.FindFirst([Windows.Automation.TreeScope]::Descendants,(New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty,'正式词条列表')))
    if($list -and $list.Current.IsEnabled){
     $item=$list.FindFirst([Windows.Automation.TreeScope]::Descendants,(New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::ListItem)))
     if($item -or $Scenario -eq 'memory'){$ready=$true;break}
    }
   }
   Start-Sleep -Milliseconds 50
  }
  if(!$ready){throw 'No interactive first row within 60 seconds'}
  $times+=$watch.Elapsed.TotalMilliseconds
  if($Scenario -eq 'memory'){
   for($m=0;$m -lt 20;$m++){$memory+=Measure-ProcessTree $p.Id;Start-Sleep -Milliseconds 500}
  }
  if($Scenario -eq 'measure'){
   $output=Join-Path $DataRoot 'frontend.jsonl'
   while($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds){
    if($p.HasExited){throw 'Measurement app exited early'}
    $memory+=Measure-ProcessTree $p.Id
    $memory[-1]|ConvertTo-Json -Compress|Add-Content -Encoding UTF8 (Join-Path $DataRoot 'memory-samples.jsonl')
    if((Test-Path $output) -and (Get-Content $output -Tail 1) -match '"case":"complete"'){break}
    Start-Sleep -Milliseconds 500
   }
   if($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds){throw 'Measurement timeout'}
  }
 }finally{
  if(!$p.HasExited){$p.CloseMainWindow()|Out-Null;if(!$p.WaitForExit(10000)){throw "Close failed; process $($p.Id) remains running"}}
 }
}
$sorted=@($times|Sort-Object)
@{scenario=$Scenario;samples=$runs;p50_ms=$sorted[[Math]::Ceiling($runs*.5)-1];p95_ms=$sorted[[Math]::Ceiling($runs*.95)-1];all_ms=$times;memory_samples=$memory;working_peak_bytes=($memory|ForEach-Object {$_['working_bytes']}|Measure-Object -Maximum).Maximum;private_peak_bytes=($memory|ForEach-Object {$_['private_bytes']}|Measure-Object -Maximum).Maximum;executable_sha256=(Get-FileHash $Executable -Algorithm SHA256).Hash;cache='OS file cache uncontrolled'}|ConvertTo-Json -Depth 3|Set-Content -Encoding UTF8 (Join-Path $DataRoot "$Scenario-results.json")
