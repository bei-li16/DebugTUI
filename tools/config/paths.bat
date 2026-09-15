@echo off
rem Connection settings only. Executables are always taken from this tools copy.
if not defined JLINK_DEVICE set "JLINK_DEVICE=STM32F429IG"
if not defined JLINK_IF set "JLINK_IF=SWD"
if not defined JLINK_SPEED set "JLINK_SPEED=4000"
if not defined GDB_PORT set "GDB_PORT=3333"
exit /b 0
