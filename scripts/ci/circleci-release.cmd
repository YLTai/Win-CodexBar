@echo on
setlocal

set "CARGO_BUILD_TARGET=x86_64-pc-windows-msvc"
set "ASSETS_DIR=C:\code\Win-CodexBar-release\assets"
if not exist "%ASSETS_DIR%" mkdir "%ASSETS_DIR%"
set "RELEASE_LOG=%ASSETS_DIR%\circleci-release.log"

powershell.exe -NoLogo -ExecutionPolicy Bypass -File scripts\ci\circleci-release.ps1 > "%RELEASE_LOG%" 2>&1
set "RELEASE_EXIT=%ERRORLEVEL%"
powershell.exe -NoLogo -Command "if (Test-Path '%RELEASE_LOG%') { Get-Content '%RELEASE_LOG%' -Tail 250 }"
if not "%RELEASE_EXIT%"=="0" exit %RELEASE_EXIT%

call scripts\ci\assert-release-assets.cmd
if errorlevel 1 exit %ERRORLEVEL%

exit 0
