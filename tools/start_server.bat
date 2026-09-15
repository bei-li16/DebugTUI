@echo off
setlocal DisableDelayedExpansion
call "%~dp0config\paths.bat"
set "PATH=%SystemRoot%\System32;%SystemRoot%"
if not exist "%~dp0bin\jlink\JLinkGDBServerCL.exe" goto missing
if not exist "%~dp0bin\jlink\JLink_x64.dll" goto missing
echo Starting %JLINK_DEVICE% on localhost:%GDB_PORT%. Keep this window open.
"%~dp0bin\jlink\JLinkGDBServerCL.exe" -device %JLINK_DEVICE% -if %JLINK_IF% -speed %JLINK_SPEED% -port %GDB_PORT% -select USB -localhostonly -nogui -halt -singlerun
exit /b %errorlevel%

:missing
echo ERROR: Required J-Link files are missing from tools\bin\jlink. 1>&2
exit /b 2
