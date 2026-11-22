$ErrorActionPreference='Stop'
. "$PSScriptRoot/update.ps1"
function Assert($ok,$message){if(!$ok){throw $message}}
$release=@{tag_name='v0.1.18';draft=$false;prerelease=$false;assets=@(@{name='QuickPeek-0.1.18-windows-x64-setup.exe';browser_download_url='https://github.com/Nex-Z/QuickPeek/releases/download/v0.1.18/QuickPeek-0.1.18-windows-x64-setup.exe';digest=('sha256:'+'a'*64)})}
Assert ((Get-UpdateAsset $release ([version]'0.1.17')).name -eq $release.assets[0].name) 'upgrade not found'
Assert (!(Get-UpdateAsset $release ([version]'0.1.18'))) 'equal version'
Assert (!(Get-UpdateAsset $release ([version]'0.1.100'))) 'downgrade'
foreach($field in 'draft','prerelease'){
    $release[$field]=$true
    Assert (!(Get-UpdateAsset $release ([version]'0.1.17'))) $field
    $release[$field]=$false
}
$release.assets[0].browser_download_url='https://example.org/setup.exe'
$rejected=$false
try {Get-UpdateAsset $release ([version]'0.1.17')} catch {$rejected=$true}
Assert $rejected 'wrong repository accepted'
function Invoke-WebRequest {param($Uri,$OutFile,[switch]$UseBasicParsing,$TimeoutSec) [IO.File]::WriteAllText($OutFile,'incomplete download')}
$rejected=$false
try {Install-Update $release.assets[0] 'unused' $null} catch {$rejected=$_.Exception.Message -like '*SHA-256*'}
Assert $rejected 'corrupt payload accepted'
'Passed version, stable release, origin, and download integrity checks.'
