param([Parameter(Mandatory=$true)][int]$AppProcessId,[int]$Samples=120)
$ErrorActionPreference='Stop'
if($Samples -lt 1){throw 'Samples must be positive'}
if(!(Get-Process -Id $AppProcessId -ErrorAction SilentlyContinue)){throw 'Application process does not exist'}
$measurements=@()
for($i=0;$i -lt $Samples;$i++){
 $all=@(Get-CimInstance Win32_Process|Select-Object ProcessId,ParentProcessId)
 $ids=[Collections.Generic.HashSet[int]]::new();[void]$ids.Add($AppProcessId)
 do{$before=$ids.Count;$all|Where-Object {$ids.Contains([int]$_.ParentProcessId)}|ForEach-Object {[void]$ids.Add([int]$_.ProcessId)}}while($before -lt $ids.Count)
 $processes=@($ids|ForEach-Object {Get-Process -Id $_ -ErrorAction SilentlyContinue})
 if(!$processes.Count){break}
 $measurements+=@{working_bytes=($processes|Measure-Object WorkingSet64 -Sum).Sum;private_bytes=($processes|Measure-Object PrivateMemorySize64 -Sum).Sum;processes=$processes.Count}
 Start-Sleep -Milliseconds 1000
}
@{samples=$measurements.Count;working_peak_bytes=($measurements|ForEach-Object {$_['working_bytes']}|Measure-Object -Maximum).Maximum;private_peak_bytes=($measurements|ForEach-Object {$_['private_bytes']}|Measure-Object -Maximum).Maximum}|ConvertTo-Json
