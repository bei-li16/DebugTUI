@echo off
rem Install these tools into PROJECT\.vscode and point PROJECT\debug.toml at them.
rem Usage: install.bat PROJECT_DIR [probe: auto, jlink, cmsis-dap or stlink] [ELF]
setlocal
if "%~1"=="" goto usage
set "PS=powershell.exe"
where pwsh.exe >nul 2>nul && set "PS=pwsh.exe"
"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
exit /b %errorlevel%

:usage
echo Usage: %~nx0 PROJECT_DIR [auto^|jlink^|cmsis-dap^|stlink] [ELF]
echo   Copies GDB, OpenOCD and the STM32F429 profiles into PROJECT_DIR\.vscode
echo   and creates or updates PROJECT_DIR\debug.toml to use them.
exit /b 2
