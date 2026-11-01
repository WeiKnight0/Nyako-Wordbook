param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$DataRoot)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$Executable=(Resolve-Path $Executable).Path
$DataRoot=(Resolve-Path $DataRoot).Path
if(!(Test-Path (Join-Path $DataRoot '.nyako-acceptance'))){throw 'Missing acceptance marker'}
$env:NYAKO_ACCEPTANCE_ROOT=$DataRoot
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
$source=Join-Path $DataRoot 'input.json'
@{format='nyako-wordbook';version=2;source_namespace='native-exchange';exported_at='2026-11-01T00:00:00Z';words=@(@{external_id='native-cat';kanji='猫';kana='ねこ';senses=@(@{meaning='原生导入猫'})})}|ConvertTo-Json -Depth 8|Set-Content -Encoding UTF8 $source
function Window {
 $condition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty,$script:process.Id)
 $script:window=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,$condition)
}
function Element([string]$Name,[Windows.Automation.ControlType]$Type){
 $watch=[Diagnostics.Stopwatch]::StartNew()
 do{
  Window
  if($script:window){
   $conditions=[Windows.Automation.Condition[]]@((New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty,$Name)),(New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty,$Type)))
   $found=$script:window.FindFirst([Windows.Automation.TreeScope]::Descendants,(New-Object Windows.Automation.AndCondition(,$conditions)))
   if($found){return $found}
  }
  if($script:process.HasExited){throw 'App exited'}
  Start-Sleep -Milliseconds 100
 }while($watch.Elapsed.TotalSeconds -lt 15)
 if($script:window){$script:window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|ForEach-Object {"$($_.Current.ControlType.ProgrammaticName): $($_.Current.Name)"}|Set-Content -Encoding UTF8 (Join-Path $DataRoot 'exchange-failure.txt')}
 throw "Missing $Name ($Type)"
}
function Click([string]$Name){
 $watch=[Diagnostics.Stopwatch]::StartNew()
 do {$button=Element $Name ([Windows.Automation.ControlType]::Button);if($button.Current.IsEnabled){break};Start-Sleep -Milliseconds 100}while($watch.Elapsed.TotalSeconds -lt 15)
 if(!$button.Current.IsEnabled){throw "Button disabled: $Name"}
 $button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke();Start-Sleep -Milliseconds 300
}
function Fill([string]$Name,[string]$Value){$input=Element $Name ([Windows.Automation.ControlType]::Edit);$input.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($Value);Start-Sleep -Milliseconds 150}
function Start-App {$script:process=Start-Process $Executable -PassThru;[void](Element '数据管理' ([Windows.Automation.ControlType]::Button));Click '数据管理'}
function Close-App {[void]$script:process.CloseMainWindow();if(!$script:process.WaitForExit(10000)){throw 'Close timeout'}}
function File-Dialog([string]$Path){
 if(-not ('NyakoDialog' -as [type])){Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NyakoDialog {
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window,int id);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr window,uint message,IntPtr parameter,string text);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window,uint message,IntPtr parameter,IntPtr value);
}
'@}
 Start-Sleep -Milliseconds 700
 $condition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty,$script:process.Id)
 $dialogs=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,$condition)
 $dialog=$dialogs|Where-Object {$_.Current.Name -in @('另存为','打开')}|Select-Object -First 1
 if(!$dialog){throw 'Native file dialog not found'}
 $handle=[IntPtr]$dialog.Current.NativeWindowHandle
 $hostCondition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::AutomationIdProperty,'FileNameControlHost')
 $hostControl=$dialog.FindFirst([Windows.Automation.TreeScope]::Descendants,$hostCondition)
 if($hostControl){$edit=[NyakoDialog]::GetDlgItem([IntPtr]$hostControl.Current.NativeWindowHandle,1001)}
 else {
  $editCondition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ClassNameProperty,'Edit')
  $editControl=$dialog.FindAll([Windows.Automation.TreeScope]::Descendants,$editCondition)|Where-Object {$_.Current.AutomationId -in @('1001','1148')}|Select-Object -First 1
  if(!$editControl){$editControl=$dialog.FindFirst([Windows.Automation.TreeScope]::Descendants,(New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::AutomationIdProperty,'1148')))}
  if(!$editControl){throw "Open dialog filename control unavailable: $($dialog.Current.Name)"}
  $edit=[IntPtr]$editControl.Current.NativeWindowHandle
 }
 if($edit -eq [IntPtr]::Zero){throw 'File name control not found'}
 [void][NyakoDialog]::SendMessage($edit,12,[IntPtr]::Zero,$Path)
 [void][NyakoDialog]::PostMessage($handle,273,[IntPtr]1,[IntPtr]::Zero)
 Start-Sleep -Milliseconds 1500
}
Start-App
try{
 Click '选择文件'
 File-Dialog $source
 Fill '新建词库' '原生交换词库'
 Click '全量校验并预览'
 [void](Element '确认并原子提交' ([Windows.Automation.ControlType]::Button))
 Click '确认并原子提交'
 Start-Sleep -Milliseconds 700
 Click '← 返回词库'
 $selected=Element '选择 猫' ([Windows.Automation.ControlType]::CheckBox)
 $selected.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 Click '导出所选词条'
 $json=Join-Path $DataRoot ('export-'+[guid]::NewGuid().ToString()+'.json')
 Click '导出 JSON'
 File-Dialog $json
 if(!(Test-Path $json)){throw 'Native JSON export did not publish'}
 $document=Get-Content -Raw -Encoding UTF8 $json|ConvertFrom-Json
 if($document.words.Count -lt 1){throw 'Native JSON export empty'}
 $xlsx=Join-Path $DataRoot ('export-'+[guid]::NewGuid().ToString()+'.xlsx')
 Click '导出 Excel'
 File-Dialog $xlsx
 if(!(Test-Path $xlsx)){throw 'Native Excel export did not publish'}
 Click '导入'
 Fill '文件路径（也可拖入）' $xlsx
 Click '查看样本'
 Click '全量校验并预览'
 Click '确认并原子提交'
 Click '← 返回词库'
 Click '数据管理'
 $template=Join-Path $DataRoot 'template.xlsx'
 Click '下载 Excel 模板'
 File-Dialog $template
 if(!(Test-Path $template)){throw 'Excel template was not published'}
 Click '备份与恢复'
 $backup=Join-Path $DataRoot ('native-'+[guid]::NewGuid().ToString()+'.nyakobak')
 Click '创建完整备份'
 File-Dialog $backup
 if(!(Test-Path $backup)){throw 'Native backup dialog did not publish output'}
 Click '选择备份并完整校验'
 File-Dialog $backup
 $confirm=Element '我确认覆盖当前新版数据库中的词库和学习状态' ([Windows.Automation.ControlType]::CheckBox)
 $confirm.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 Click '安全备份后安排恢复'
 Start-Sleep -Milliseconds 1000
 if(!(Test-Path (Join-Path $DataRoot 'restore-journal.json'))){throw 'Restore was not scheduled'}
 Close-App
 Start-App
 if(Test-Path (Join-Path $DataRoot 'restore-journal.json')){throw 'Restore journal was not finalized'}
 Click '← 返回词库'
 [void](Element '编辑词库 原生交换词库' ([Windows.Automation.ControlType]::Button))
 [void](Element '搜索真实词条' ([Windows.Automation.ControlType]::Edit))
 Fill '搜索真实词条' '猫'
 Start-Sleep -Milliseconds 500
 [void](Element '选择 猫' ([Windows.Automation.ControlType]::CheckBox))
 Close-App
 Start-App
 Click '← 返回词库'
 Fill '搜索真实词条' '猫'
 Start-Sleep -Milliseconds 500
 [void](Element '选择 猫' ([Windows.Automation.ControlType]::CheckBox))
 Close-App
 @{case='native/import-backup-dialog-validate-restore-search-reopen';passed=$true}|ConvertTo-Json|Set-Content -Encoding UTF8 (Join-Path $DataRoot 'exchange-results.json')
}finally{if(!$script:process.HasExited){[void]$script:process.CloseMainWindow()}}
