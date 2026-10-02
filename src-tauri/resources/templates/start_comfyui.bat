@echo off
title Z-Image Turbo (ComfyUI)
cd /d "{{ROOT}}/ComfyUI"
echo Starting ComfyUI at http://127.0.0.1:8188 ...
".venv\Scripts\python.exe" -u main.py --listen 127.0.0.1 --port 8188 --enable-cors-header http://tauri.localhost
pause
