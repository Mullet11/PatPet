@echo off
rem Double-click to restart PatPet with logging on. Reproduce the problem, then tell Claude
rem roughly when it happened; the log is at %TEMP%\patpet.log (times are in milliseconds).
cd /d "%~dp0.."
if exist "target\release\patpet.exe" "target\release\patpet.exe" --quit
for /l %%i in (1,1,10) do (
    tasklist /fi "imagename eq patpet.exe" 2>nul | find /i "patpet.exe" >nul || goto closed
    timeout /t 1 /nobreak >nul
)
taskkill /im patpet.exe /f >nul 2>&1
:closed
del "%TEMP%\patpet.log" >nul 2>&1
set PATPET_DEBUG=1
start "" "target\release\patpet.exe"
echo PatPet is running with logging on. Log: %TEMP%\patpet.log
timeout /t 4 >nul
