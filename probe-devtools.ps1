$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
$p = Start-Process -FilePath "src-tauri\target\release\app.exe" -PassThru
Start-Sleep 6
try {
  $v = Invoke-RestMethod http://localhost:9222/json/version
  Write-Host "DEVTOOLS OK: $($v.Browser)"
} catch {
  Write-Host "NO DEVTOOLS: $($_.Exception.Message)"
}
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
