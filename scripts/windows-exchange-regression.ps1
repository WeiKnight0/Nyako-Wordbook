param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$DataRoot)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$Executable=(Resolve-Path $Executable).Path
$DataRoot=(Resolve-Path $DataRoot).Path
if(!(Test-Path (Join-Path $DataRoot '.nyako-acceptance') -PathType Leaf)){throw 'Missing acceptance marker'}
$env:NYAKO_ACCEPTANCE_ROOT=$DataRoot
Remove-Item Env:NYAKO_ACCEPTANCE_MEASURE -ErrorAction SilentlyContinue
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
$run=[guid]::NewGuid().ToString()
$library='原生交换词库-'+$run
$roundtrip='Excel往返词库-'+$run
$source=Join-Path $DataRoot ("input-$run.json")
$cat=@{external_id='native-cat';kanji='受入交換猫';kana='うけいれこうかんねこ';parts_of_speech=@('noun');jlpt='N5';pitch_accents=@(@{downstep=0},@{downstep=2});senses=@(@{meaning='原生导入猫'},@{meaning='备份中的第二义项'});examples=@(@{jp='猫がいます。';cn='有一只猫。';sense_index=0},@{jp='猫を見た。';cn='看见了猫。';sense_index=1});note='恢复基准备注'}
$dog=@{external_id='native-dog';kanji='受入交換犬';kana='うけいれこうかんいぬ';parts_of_speech=@('noun');jlpt=$null;pitch_accents=@();senses=@(@{meaning='原生导入犬'});examples=@();note='不应出现在单选导出'}
@{format='nyako-wordbook';version=2;source_namespace="native-exchange-$run";exported_at='2026-11-01T00:00:00Z';words=@($cat,$dog)}|ConvertTo-Json -Depth 8|Set-Content -Encoding UTF8 $source
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
function Fill([string]$Name,[string]$Value){
 (Element $Name ([Windows.Automation.ControlType]::Edit)).GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
 Start-Sleep -Milliseconds 150
 if((Element $Name ([Windows.Automation.ControlType]::Edit)).GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -cne $Value){throw "Value mismatch: $Name"}
}
function Start-App {$script:process=Start-Process $Executable -PassThru;[void](Element '搜索真实词条' ([Windows.Automation.ControlType]::Edit))}
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
function Assert-Text([string]$Pattern){
 $watch=[Diagnostics.Stopwatch]::StartNew()
 do{
  Window
  $text=($script:window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|ForEach-Object {$_.Current.Name}) -join ' '
  if(($text -replace '\s+',' ') -match $Pattern){return}
  if($script:process.HasExited){throw 'App exited during assertion'}
  Start-Sleep -Milliseconds 100
 }while($watch.Elapsed.TotalSeconds -lt 15)
 throw "Missing expected UI text: $Pattern; actual: $text"
}
function Result([string]$Message,[string]$Path=''){
 [void](Element '知道了' ([Windows.Automation.ControlType]::Button))
 Assert-Text '操作完成'
 Assert-Text ([regex]::Escape($Message))
 # 原生文件对话框会把 Windows 8.3 路径展开，文件名与实际输出内容才是稳定断言。
 if($Path){Assert-Text ([regex]::Escape([IO.Path]::GetFileName($Path)));if(!(Test-Path $Path -PathType Leaf)){throw "Output missing: $Path"}}
 Click '知道了'
}
function Preview([int]$Count){
 Click '全量校验并预览'
 [void](Element '确认并原子提交' ([Windows.Automation.ControlType]::Button))
 Assert-Text "共 $Count 行 $Count 条有效 0 条错误"
}
function Commit([string]$Message){Click '确认并原子提交';Click '确认执行';Result $Message}
function Select-Word([string]$Writing){
 Fill '搜索真实词条' $Writing
 Start-Sleep -Milliseconds 350
 Assert-Text '(?<!\d)1 个匹配词条'
 $selection=(Element "选择 $Writing" ([Windows.Automation.ControlType]::CheckBox)).GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
 if($selection.Current.ToggleState -ne [Windows.Automation.ToggleState]::On){$selection.Toggle()}
 Click '导出所选词条'
}
function Word-Content($Word){
 [ordered]@{kanji=$Word.kanji;kana=$Word.kana;parts_of_speech=@($Word.parts_of_speech);jlpt=$Word.jlpt;pitch_accents=@($Word.pitch_accents|ForEach-Object {[ordered]@{downstep=$_.downstep}});senses=@($Word.senses|ForEach-Object {[ordered]@{meaning=$_.meaning}});examples=@($Word.examples|ForEach-Object {[ordered]@{jp=$_.jp;cn=$_.cn;sense_index=$_.sense_index}});note=$Word.note}|ConvertTo-Json -Depth 8 -Compress
}
function Assert-Export([string]$Path,$Expected){
 $document=Get-Content -Raw -Encoding UTF8 $Path|ConvertFrom-Json
 if($document.format -cne 'nyako-wordbook' -or $document.version -ne 2 -or !$document.source_namespace -or !$document.exported_at -or @($document.words).Count -ne 1){throw "Invalid single-word JSON export: $Path"}
 if((Word-Content $document.words[0]) -cne (Word-Content $Expected)){throw "Exported word content mismatch: $Path"}
 if(!$document.words[0].external_id){throw 'Exported stable word identity missing'}
 return $document
}
function Export-Word([string]$Label,$Expected){
 Select-Word $Expected.kanji
 $output=Join-Path $DataRoot ("$Label-$run.json")
 Click '导出 JSON';File-Dialog $output;Result 'JSON 已导出：' $output
 $document=Assert-Export $output $Expected
 Click '← 返回词库'
 return $document
}
function Read-ZipXml($Archive,[string]$Name){
 $reader=[IO.StreamReader]::new($Archive.GetEntry($Name).Open())
 try{return [xml]$reader.ReadToEnd()}finally{$reader.Dispose()}
}
function Assert-Workbook([string]$Path,[object[]]$ExpectedRow){
 Add-Type -AssemblyName System.IO.Compression.FileSystem
 $archive=[IO.Compression.ZipFile]::OpenRead($Path)
 try{
  $book=Read-ZipXml $archive 'xl/workbook.xml'
  if(@($book.workbook.sheets.sheet).Count -ne 1 -or $book.workbook.sheets.sheet.name -cne '词条'){throw 'Expected exactly one worksheet named 词条'}
  $strings=@()
  if($archive.GetEntry('xl/sharedStrings.xml')){$strings=@((Read-ZipXml $archive 'xl/sharedStrings.xml').sst.si|ForEach-Object {($_.SelectNodes('.//*[local-name()="t"]')|ForEach-Object {$_.InnerText}) -join ''})}
  $rows=@((Read-ZipXml $archive 'xl/worksheets/sheet1.xml').worksheet.sheetData.row)
  if($rows.Count -ne 2){throw "Expected header and exactly one Excel word: $Path"}
  $expected=@(@('词条标识','写法','假名','释义','词性','音调','JLPT','日文例句','例句中文翻译','备注'),$ExpectedRow)
  0..1|ForEach-Object {
   $index=$_
   $cells=@($rows[$index].c|ForEach-Object {if($_.t -eq 's'){$strings[[int]$_.v]}elseif($_.t -eq 'inlineStr'){$_.is.InnerText}else{[string]$_.v}})
   if(($cells|ConvertTo-Json -Compress) -cne ($expected[$index]|ConvertTo-Json -Compress)){throw "Excel row $index content mismatch: $Path"}
  }
 }finally{$archive.Dispose()}
}
function Assert-Restored([string]$Label){
 if(Test-Path (Join-Path $DataRoot 'restore-journal.json')){throw 'Restore journal was not finalized'}
 [void](Element "编辑词库 $library" ([Windows.Automation.ControlType]::Button))
 [void](Element "编辑词库 $roundtrip" ([Windows.Automation.ControlType]::Button))
 Fill '搜索真实词条' '受入交換'
 Assert-Text '(?<!\d)2 个匹配词条'
 Assert-Text ([regex]::Escape("$library 2"))
 Assert-Text ([regex]::Escape("$roundtrip 1"))
 $restored=Export-Word $Label $cat
 if($restored.source_namespace -cne $script:original.source_namespace -or $restored.words[0].external_id -cne $script:original.words[0].external_id){throw 'Restore changed stable export identity'}
 [void](Export-Word "$Label-dog" $dog)
 Fill '搜索真实词条' '受入恢复后增词'
 Assert-Text '(?<!\d)0 个匹配词条'
 Assert-Text '没有词条。可新增词条或清除筛选条件。'
}
Start-App
try{
 Fill '搜索真实词条' '受入交換'
 Assert-Text '(?<!\d)0 个匹配词条'
 Fill '搜索真实词条' '受入恢复后增词'
 Assert-Text '(?<!\d)0 个匹配词条'
 Click '数据管理'
 Click '选择文件';File-Dialog $source
 Fill '新建词库' $library
 Preview 2
 Commit '导入完成：新增 2 条，更新 0 条，未变化 0 条，跳过 0 条。'
 Click '← 返回词库'
 Assert-Text ([regex]::Escape("$library 2"))
 Select-Word $cat.kanji
 $json=Join-Path $DataRoot ("export-$run.json")
 Click '导出 JSON';File-Dialog $json;Result 'JSON 已导出：' $json
 $script:original=Assert-Export $json $cat
 $xlsx=Join-Path $DataRoot ("export-$run.xlsx")
 Click '导出 Excel';File-Dialog $xlsx;Result 'Excel 已导出：' $xlsx
 Assert-Workbook $xlsx @($script:original.words[0].external_id,$cat.kanji,$cat.kana,"1. 原生导入猫`n2. 备份中的第二义项",'noun','0/2','N5',"1. 猫がいます。`n2. [义项2] 猫を見た。","1. 有一只猫。`n2. 看见了猫。",$cat.note)
 Click '导入'
 Fill '文件路径（也可拖入）' $xlsx
 Fill '来源标识' $script:original.source_namespace
 Fill '新建词库' $roundtrip
 Click '查看样本'
 Assert-Text '备份中的第二义项'
 Preview 1
 Click '相同内容复用'
 Commit '导入完成：新增 0 条，更新 0 条，未变化 1 条，跳过 0 条。'
 Click '← 返回词库'
 Assert-Text ([regex]::Escape("$roundtrip 1"))
 [void](Export-Word 'excel-roundtrip' $cat)
 Click '数据管理'
 $template=Join-Path $DataRoot ("template-$run.xlsx")
 Click '下载 Excel 模板';File-Dialog $template;Result 'Excel 模板已保存。'
 Assert-Workbook $template @('sample-cat-001','猫','ねこ','猫','noun','1','N5','猫がいます。','有一只猫。','这是示例词条，请按需修改或删除；新增词条使用新的唯一标识。')
 Click '备份与恢复'
 $backup=Join-Path $DataRoot ("native-$run.nyakobak")
 Click '创建完整备份';File-Dialog $backup;Result '完整备份已生成并校验：' $backup
 if(!(Test-Path $backup -PathType Leaf) -or (Get-Item $backup).Length -le 0){throw 'Native backup was not published'}
 # 恢复前必须证明备份后的修改已持久化，避免用未变化的数据产生假通过。
 Click '← 返回词库'
 Fill '搜索真实词条' $cat.kanji
 Assert-Text '(?<!\d)1 个匹配词条'
 Assert-Text '备份中的第二义项'
 Click '编辑词条'
 Fill '义项 1' '备份后已修改的义项'
 Fill '备注' '备份后持久修改'
 Click '保存词条';Click '收回词条详情'
 Click '新增词条'
 Fill '写法（必填）' '受入恢复后增词'
 Fill '假名（必填）' 'うけいれふくごぞうし'
 Fill '义项 1' '只应存在于恢复前的新增词条'
 Click '保存词条'
 Close-App;Start-App
 $changed=$cat.Clone();$changed.senses=@(@{meaning='备份后已修改的义项'},@{meaning='备份中的第二义项'});$changed.note='备份后持久修改'
 [void](Export-Word 'persisted-after-backup' $changed)
 [void](Export-Word 'persisted-new-word' @{kanji='受入恢复后增词';kana='うけいれふくごぞうし';parts_of_speech=@();jlpt=$null;pitch_accents=@();senses=@(@{meaning='只应存在于恢复前的新增词条'});examples=@();note=''})
 Click '数据管理';Click '备份与恢复'
 Click '选择备份并完整校验';File-Dialog $backup
 (Element '我确认覆盖当前新版数据库中的词库和学习状态' ([Windows.Automation.ControlType]::CheckBox)).GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 Click '安全备份后安排恢复';Click '确认执行';Result '恢复已安排。请关闭并重新启动应用。'
 if(!(Test-Path (Join-Path $DataRoot 'restore-journal.json') -PathType Leaf)){throw 'Restore was not scheduled'}
 Close-App;Start-App
 Assert-Restored 'restored-first-restart'
 Close-App;Start-App
 Assert-Restored 'restored-second-restart'
 Close-App
 @{case='native/exact-import-selected-json-excel-roundtrip-template/persisted-mutation/restore-two-restarts';passed=$true;imported=2;selected_export=1;excel_unchanged=1;source=$source;backup=$backup;restored_restarts=2;executable_sha256=(Get-FileHash $Executable -Algorithm SHA256).Hash}|ConvertTo-Json|Set-Content -Encoding UTF8 (Join-Path $DataRoot 'exchange-results.json')
}finally{if(!$script:process.HasExited){[void]$script:process.CloseMainWindow()}}
