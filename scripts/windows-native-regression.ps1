param(
 [Parameter(Mandatory=$true)][string]$Executable,
 [Parameter(Mandatory=$true)][string]$DataRoot,
 [switch]$DoubleScale
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class NyakoWindow {
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int width,int height,uint flags);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
}
'@
$Executable=(Resolve-Path $Executable).Path
$DataRoot=(Resolve-Path $DataRoot).Path
if(!(Test-Path (Join-Path $DataRoot '.nyako-acceptance'))){throw 'Missing acceptance marker'}
$env:NYAKO_ACCEPTANCE_ROOT=$DataRoot
Remove-Item Env:NYAKO_ACCEPTANCE_MEASURE -ErrorAction SilentlyContinue
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
if($DoubleScale){$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS+=' --force-device-scale-factor=2'}
$log=@()
function Find-Element([string]$Name,[Windows.Automation.ControlType]$Type){
 $conditions=[Windows.Automation.Condition[]]@(
  (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty,$Name)),
  (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty,$Type))
 )
 return $script:window.FindFirst([Windows.Automation.TreeScope]::Descendants,(New-Object Windows.Automation.AndCondition(,$conditions)))
}
function Wait-Element([string]$Name,[Windows.Automation.ControlType]$Type){
 $watch=[Diagnostics.Stopwatch]::StartNew()
 do{
  if($script:process.HasExited){throw "App exited while waiting for $Name"}
  $condition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty,$script:process.Id)
  $script:window=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,$condition)
  $element=if($script:window){Find-Element $Name $Type}else{$null}
  if($element){return $element}
  Start-Sleep -Milliseconds 100
 }while($watch.Elapsed.TotalSeconds -lt 15)
 $snapshot=@($script:window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|ForEach-Object {"$($_.Current.ControlType.ProgrammaticName): $($_.Current.Name)"})
 $snapshot|Set-Content -Encoding UTF8 (Join-Path $DataRoot 'native-failure.txt')
 throw "Missing $Name ($Type), process $($script:process.Id); see native-failure.txt"
}
function Click([string]$Name){
 $element=Wait-Element $Name ([Windows.Automation.ControlType]::Button)
 if(!$element.Current.IsEnabled){throw "Disabled $Name"}
 $element.SetFocus()
 $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
 Start-Sleep -Milliseconds 250
}
function Fill([string]$Name,[string]$Value){
 $element=Wait-Element $Name ([Windows.Automation.ControlType]::Edit)
 $element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
 Start-Sleep -Milliseconds 200
 $actual=(Wait-Element $Name ([Windows.Automation.ControlType]::Edit)).GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value
 if($actual -ne $Value){throw "Value mismatch for $Name"}
}
function Start-App {
 Start-Sleep -Milliseconds 500
 $script:process=Start-Process $Executable -PassThru
 $condition=New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty,$script:process.Id)
 $watch=[Diagnostics.Stopwatch]::StartNew()
 do{
  $script:window=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,$condition)
  if($script:window){break}
  if($script:process.HasExited){throw 'App exited before window appeared'}
  Start-Sleep -Milliseconds 100
 }while($watch.Elapsed.TotalSeconds -lt 30)
 if(!$script:window){throw 'Window timeout'}
 [void](Wait-Element '新增词条' ([Windows.Automation.ControlType]::Button))
 [void][NyakoWindow]::SetWindowPos($script:window.Current.NativeWindowHandle,[IntPtr]::Zero,20,20,1000,700,4)
}
function Close-App {
 [void]$script:process.CloseMainWindow()
 if(!$script:process.WaitForExit(10000)){throw 'Window did not close'}
}
Start-App
try{
 foreach($name in @('新建词库','新增词条','展开详情','加载下一页')){
  $element=Wait-Element $name ([Windows.Automation.ControlType]::Button)
  $element.SetFocus()
  Start-Sleep -Milliseconds 150
  $element=Wait-Element $name ([Windows.Automation.ControlType]::Button)
  $rect=$element.Current.BoundingRectangle
  $windowRect=$script:window.Current.BoundingRectangle
  if($element.Current.IsOffscreen -or $rect.Width -le 0 -or $rect.Height -le 0 -or !$windowRect.Contains($rect)){throw "Button outside window: $name"}
  [void][NyakoWindow]::SetCursorPos([int]($rect.Left+$rect.Width/2),[int]($rect.Top+$rect.Height/2))
  [NyakoWindow]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
  [NyakoWindow]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 300
  $log+=@{case="mouse/$name";bounds=@($rect.Left,$rect.Top,$rect.Width,$rect.Height);double_scale=[bool]$DoubleScale}
  if($name -in @('新建词库','新增词条')){[void](Wait-Element '取消' ([Windows.Automation.ControlType]::Button));Click '取消'}
 }
 Click '新增词条'
 Fill '假名（必填）' 'かいきてすと'
 Fill '义项 1' '原生回归首义项'
 Click '新增义项'
 Fill '义项 2' '第二义项'
 Click '新增例句'
 Fill '日文例句' '毎日働く'
 Fill '中文翻译（可空）' '每天工作'
 Fill '新标签名称' '回归标签'
 Click '明确创建标签'
 foreach($library in @('基准主库','共享词库')){
  $box=Wait-Element $library ([Windows.Automation.ControlType]::CheckBox)
  $box.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 }
 Click '保存词条'
 Fill '搜索真实词条' 'かいきてすと'
 Start-Sleep -Milliseconds 500
 $selection=Wait-Element '选择 かいきてすと' ([Windows.Automation.ControlType]::CheckBox)
 $selection.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 $target=Wait-Element '目标词库' ([Windows.Automation.ControlType]::ComboBox)
 $target.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
 $option=Wait-Element '共享词库' ([Windows.Automation.ControlType]::ListItem)
 $option.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
 Click '移出词库'
 $selection=Wait-Element '选择 かいきてすと' ([Windows.Automation.ControlType]::CheckBox)
 $selection.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 Click '加入词库'
 $selection=Wait-Element '选择 かいきてすと' ([Windows.Automation.ControlType]::CheckBox)
 $selection.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
 Click '永久删除词条…'
 Click '取消'
 Click '永久删除词条…'
 Click '我已了解，继续确认'
 Click '确认执行'
 Fill '搜索真实词条' ''
 Start-Sleep -Milliseconds 500
 $log+=@{case='word/create/two-senses/example/tag/shared/batch-remove-add/delete-cancel/two-confirmations';passed=$true}
 Click '新建词库'
 Fill '词库名称' '回归词库甲'
 Fill '描述' '原生关闭保存与重开验证'
 [void]$script:process.CloseMainWindow()
 [void](Wait-Element '保存并离开' ([Windows.Automation.ControlType]::Button))
 Click '继续编辑'
 if($script:process.HasExited){throw 'Continue editing closed the window'}
 Fill '词库名称' ('错'*101)
 Click '保存词库'
 [void](Wait-Element '词库名称' ([Windows.Automation.ControlType]::Edit))
 $text=$script:window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) | ForEach-Object {$_.Current.Name}
 if(($text -join ' ') -notmatch '草稿已保留'){throw 'Failure did not report retained draft'}
 Fill '词库名称' '回归词库甲'
 [void]$script:process.CloseMainWindow()
 Click '保存并离开'
 if(!$script:process.WaitForExit(10000)){throw 'Save-and-close did not exit'}
 $log+=@{case='close/continue/failure-retains-draft/save';passed=$true}
 Start-App
 [void](Wait-Element '编辑词库 回归词库甲' ([Windows.Automation.ControlType]::Button))
 Click '新建词库'
 Fill '词库名称' '应该放弃的草稿'
 [void]$script:process.CloseMainWindow()
 Click '放弃修改'
 if(!$script:process.WaitForExit(10000)){throw 'Discard-and-close did not exit'}
 $log+=@{case='close/discard';passed=$true}
 Start-App
 if(Find-Element '编辑词库 应该放弃的草稿' ([Windows.Automation.ControlType]::Button)){throw 'Discarded draft was saved'}
 Close-App
 $log|ConvertTo-Json -Depth 5|Set-Content -Encoding UTF8 (Join-Path $DataRoot 'native-results.json')
}finally{
 if(!$script:process.HasExited){[void]$script:process.CloseMainWindow()}
}
