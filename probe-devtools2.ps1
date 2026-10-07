$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
$p = Start-Process -FilePath "src-tauri\target\release\app.exe" -PassThru -RedirectStandardError probe-err.log -RedirectStandardOutput probe-out.log
Start-Sleep 8
if ($p.HasExited) { Write-Host "APP EXITED code=$($p.ExitCode)" } else { Write-Host "APP RUNNING pid=$($p.Id)" }
netstat -ano | Select-String ':9222'
Get-Content probe-err.log -ErrorAction SilentlyContinue | Select-Object -First 20
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
