@echo off
rem Double-click to rebuild PatPet: closes the running copy, builds, then starts it again.
cd /d "%~dp0"
title Rebuild PatPet

echo Closing PatPet if it is running...
if exist "target\release\patpet.exe" "target\release\patpet.exe" --quit
rem give it a moment to save and close, then make sure it is gone
for /l %%i in (1,1,10) do (
    tasklist /fi "imagename eq patpet.exe" 2>nul | find /i "patpet.exe" >nul || goto closed
    timeout /t 1 /nobreak >nul
)
taskkill /im patpet.exe /f >nul 2>&1
timeout /t 1 /nobreak >nul
:closed

echo Building...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1"
if errorlevel 1 (
    echo.
    echo BUILD FAILED - see the messages above.
    pause
    exit /b 1
)

echo Starting PatPet...
start "" "%~dp0target\release\patpet.exe"
echo Done.
timeout /t 2 >nul
