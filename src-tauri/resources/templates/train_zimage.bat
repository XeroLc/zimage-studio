@echo off
title Z-Image LoRA training (ai-toolkit)
cd /d "{{ROOT}}/ai-toolkit"
set MODELS_PATH={{ROOT}}/models
set HF_ENDPOINT=https://hf-mirror.net
set PYTHONIOENCODING=utf-8
".venv\Scripts\python.exe" -u run.py config/zimage_character_8gb.yml
pause
