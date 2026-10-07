Start-Process -FilePath "$env:USERPROFILE\.cargo\bin\tauri-driver.exe" -WindowStyle Hidden
Start-Sleep 2
$appPath = (Resolve-Path 'src-tauri\target\release\app.exe').Path
$body = @{
  capabilities = @{
    alwaysMatch = @{
      browserName = 'wry'
      'tauri:options' = @{ application = $appPath }
    }
  }
} | ConvertTo-Json -Depth 6
try {
  $r = Invoke-RestMethod -Uri http://localhost:4444/session -Method Post -Body $body -ContentType 'application/json'
  $r | ConvertTo-Json -Depth 8
} catch {
  $_.ErrorDetails.Message
}
