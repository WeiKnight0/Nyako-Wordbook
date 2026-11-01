param(
 [Parameter(Mandatory=$true)][string]$Installer,
 [Parameter(Mandatory=$true)][string]$InstallDirectory
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$Installer=(Resolve-Path $Installer).Path
if(![IO.Path]::IsPathRooted($InstallDirectory) -or (Test-Path $InstallDirectory)){throw 'Use a new absolute installation directory'}
$registered=@('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')|ForEach-Object {Get-ItemProperty $_ -ErrorAction SilentlyContinue}|Where-Object {$_.DisplayName -like '*NyakoWordbook*'}
if($registered){throw 'Existing NyakoWordbook installation; do not overwrite it'}
if(Get-Process nyako-wordbook -ErrorAction SilentlyContinue){throw 'Close existing NyakoWordbook before installation regression'}
$database=Join-Path $env:APPDATA 'io.nyako.wordbook\data\main.sqlite3'
$before=if(Test-Path $database){(Get-FileHash $database -Algorithm SHA256).Hash}else{$null}
$installation=Start-Process $Installer -ArgumentList @('/S',"/D=$InstallDirectory") -Wait -PassThru
if($installation.ExitCode -ne 0){throw "Installer exit $($installation.ExitCode)"}
$exe=Join-Path $InstallDirectory 'nyako-wordbook.exe'
if(!(Test-Path $exe)){throw 'Installed executable missing'}
for($i=0;$i -lt 2;$i++){
 $app=Start-Process $exe -PassThru
 try{
  $watch=[Diagnostics.Stopwatch]::StartNew()
  do{
   $app.Refresh()
   if($app.HasExited){throw 'Installed app exited early'}
   if($app.MainWindowHandle -ne [IntPtr]::Zero){break}
   Start-Sleep -Milliseconds 100
  }while($watch.Elapsed.TotalSeconds -lt 30)
  if($app.MainWindowHandle -eq [IntPtr]::Zero){throw 'No installed window'}
  if($i -eq 0){
   $second=Start-Process $exe -PassThru
   if(!$second.WaitForExit(10000)){throw 'Second instance remained running'}
   if($second.ExitCode -ne 0){throw 'Second instance failed'}
  }
  [void]$app.CloseMainWindow()
  if(!$app.WaitForExit(10000)){throw 'Installed app did not close'}
 }finally{
  if(!$app.HasExited){[void]$app.CloseMainWindow()}
 }
}
$uninstaller=Join-Path $InstallDirectory 'uninstall.exe'
if(!(Test-Path $uninstaller)){throw 'Uninstaller missing'}
$uninstall=Start-Process $uninstaller -ArgumentList '/S' -Wait -PassThru
if($uninstall.ExitCode -ne 0){throw "Uninstall exit $($uninstall.ExitCode)"}
$watch=[Diagnostics.Stopwatch]::StartNew()
while((Test-Path $exe) -and $watch.Elapsed.TotalSeconds -lt 15){Start-Sleep -Milliseconds 100}
if(Test-Path $exe){throw 'Installed executable remains after uninstall'}
$remaining=@('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')|ForEach-Object {Get-ItemProperty $_ -ErrorAction SilentlyContinue}|Where-Object {$_.DisplayName -like '*NyakoWordbook*'}
if($remaining){throw 'Uninstall registration remains'}
$after=if(Test-Path $database){(Get-FileHash $database -Algorithm SHA256).Hash}else{$null}
if($before -and $before -ne $after){throw 'Business database hash changed'}
@{installer_sha256=(Get-FileHash $Installer -Algorithm SHA256).Hash;install_exit=$installation.ExitCode;single_instance=$true;reopen=$true;uninstall_exit=$uninstall.ExitCode;database_before=$before;database_after=$after}|ConvertTo-Json
