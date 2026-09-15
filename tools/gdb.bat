@echo off
setlocal DisableDelayedExpansion
if "%~1"=="" goto usage
if not exist "%~f1" (
    echo ERROR: ELF file not found: "%~1" 1>&2
    exit /b 2
)
call "%~dp0config\paths.bat"
set "PATH=%SystemRoot%\System32;%SystemRoot%"
set "GDB_ROOT=%~dp0bin\gdb"
if not exist "%GDB_ROOT%\bin\arm-none-eabi-gdb.exe" (
    echo ERROR: Required GDB is missing from tools\bin\gdb\bin. 1>&2
    exit /b 2
)
set "GDB_ROOT=%GDB_ROOT:\=/%"
"%~dp0bin\gdb\bin\arm-none-eabi-gdb.exe" -q -nx ^
    --data-directory="%GDB_ROOT%/arm-none-eabi/share/gdb" ^
    -iex "set debug-file-directory %GDB_ROOT%/lib/debug" ^
    -iex "set auto-load scripts-directory %GDB_ROOT%/arm-none-eabi/share/gdb/auto-load" ^
    -iex "set auto-load safe-path %GDB_ROOT%/arm-none-eabi/share/gdb/auto-load" ^
    -ex "set pagination off" ^
    -ex "set remotetimeout 5" ^
    -ex "set tcp connect-timeout 5" ^
    -ex "target extended-remote localhost:%GDB_PORT%" ^
    -x "%~dp0config\connect.gdb" %*
exit /b %errorlevel%

:usage
echo Usage: gdb.bat "firmware.elf" [GDB options]
echo Start start_server.bat in another terminal first.
echo Example: gdb.bat "firmware.elf" -batch -ex "p xTickCount" -ex "monitor go" -ex "detach"
exit /b 2
