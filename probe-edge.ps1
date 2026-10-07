$driverProc = Start-Process -FilePath "$env:USERPROFILE\.cargo\bin\msedgedriver.exe" -ArgumentList '--port=9515' -PassThru -WindowStyle Hidden
Start-Sleep 2
$appPath = (Resolve-Path 'src-tauri\target\release\app.exe').Path
$body = @{
  capabilities = @{
    alwaysMatch = @{
      browserName = 'webview2'
      'ms:edgeOptions' = @{
        binary = $appPath
        args = @('--remote-debugging-port=0')
      }
    }
  }
} | ConvertTo-Json -Depth 6
try {
  $r = Invoke-RestMethod -Uri http://localhost:9515/session -Method Post -Body $body -ContentType 'application/json'
  $r | ConvertTo-Json -Depth 8
} catch {
  $_.ErrorDetails.Message
}
Get-Process app -ErrorAction SilentlyContinue | Select-Object Id, ProcessName, StartTime
Stop-Process -Id $driverProc.Id -Force -ErrorAction SilentlyContinue
