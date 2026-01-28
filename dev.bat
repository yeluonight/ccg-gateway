@echo off
echo Starting CCG Gateway Development Environment...
echo.

echo [1/2] Starting frontend dev server...
start "Frontend Dev Server" cmd /k "cd frontend && pnpm dev"

echo [2/2] Waiting for frontend to start...
timeout /t 5 /nobreak > nul

echo Starting Tauri backend...
cd src-tauri
rem set RUST_LOG=warn,ccg_gateway=warn,ccg_gateway_lib=warn
cargo run

pause
