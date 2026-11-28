@echo off
set PARTH_DB_DIR=%~dp0data
set PARTH_DB_TOKEN=reviewer-demo-token-2026
set PARTH_DB_ADDR=127.0.0.1:8787
set PARTH_DB_ALLOW_REMOTE=0
start "" /b cmd /c "ping -n 3 127.0.0.1 >nul & start http://127.0.0.1:8787/"
"%~dp0parth_db.exe"