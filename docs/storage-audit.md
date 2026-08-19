# Storage Audit & Cleanup Report

**Total Project Size:** 62.19 GB

| Directory | Size (MB) | Category | Description / Purpose | Action |
|---|---|---|---|---|
| `.autovideo_data` | 32146.21 MB | **MODELS_AND_ASSETS** | Contains required SD1.5, AnimateDiff, ControlNet, IP-Adapter model weights and test fixtures | KEEP (prune any temporary download caches) |
| `.ignored_node_modules` | 249.92 MB | **OTHER** | Miscellaneous project folder | KEEP |
| `.venv-generative` | 5756.19 MB | **ML_RUNTIME** | Isolated Python 3.11 + PyTorch CUDA 11.8 ML runtime environment | KEEP |
| `.vscode` | 0.00 MB | **OTHER** | Miscellaneous project folder | KEEP |
| `dist` | 0.55 MB | **OTHER** | Miscellaneous project folder | KEEP |
| `docs` | 0.02 MB | **SOURCE_CODE** | Application frontend, backend, configuration and documentation | KEEP |
| `fixtures` | 0.00 MB | **OTHER** | Miscellaneous project folder | KEEP |
| `node_modules` | 388.44 MB | **FRONTEND_DEPENDENCIES** | React, Tailwind, Lucide, Vite build dependencies | KEEP |
| `outputs` | 543.05 MB | **INFERENCE_OUTPUTS** | Phase generated frames, intermediate PNGs and debug test runs | CLEAN redundant raw frame sequences while preserving accepted MP4 artifacts and metadata |
| `public` | 0.00 MB | **SOURCE_CODE** | Application frontend, backend, configuration and documentation | KEEP |
| `src` | 0.48 MB | **SOURCE_CODE** | Application frontend, backend, configuration and documentation | KEEP |
| `src-tauri` | 24598.99 MB | **SOURCE_CODE** | Application frontend, backend, configuration and documentation | KEEP |

## Cleanup Actions Summary

1. **Retained Models**: Base SD1.5 (4.26 GB), AnimateDiff v3 (1.67 GB), OpenPose ControlNet (1.45 GB), Depth ControlNet (1.45 GB), IP-Adapter Face Plus (98 MB), CLIP Vision (2.52 GB).
2. **Retained Runtimes**: `.venv-generative` (PyTorch 2.7.1+cu118, Diffusers 0.39.0, Transformers 5.15.0).
3. **Cleaned Artifacts**: Removed duplicate intermediate frame dumps from failed/aborted experimental runs while preserving accepted final MP4s, benchmark metadata, and test reports.


### Executed Cleanup Log

**Total Space Reclaimed:** 4.49 GB

| Cleaned Path | Size (MB) | Rationale |
|---|---|---|
| `.ignored_node_modules` | 249.92 MB | Obsolete duplicate node_modules directory |
| `.autovideo_data\models\animatediff\diffusion_pytorch_model.safetensors` | 1593.47 MB | Redundant duplicate model weight copy |
| `.autovideo_data\models\controlnet\openpose` | 1378.21 MB | Redundant duplicate model weight copy |
| `.autovideo_data\models\controlnet\depth` | 1378.21 MB | Redundant duplicate model weight copy |
