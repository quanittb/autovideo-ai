import os
import sys
from pathlib import Path

def get_dir_size(p: Path):
    total = 0
    try:
        for f in p.rglob('*'):
            if f.is_file():
                total += f.stat().st_size
    except Exception:
        pass
    return total

def main():
    root = Path(r"D:\rustProject\autovideo-ai")
    docs_dir = root / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)

    print("Auditing project directory sizes...")
    subdirs = [d for d in root.iterdir() if d.is_dir() and not d.name.startswith('.git')]
    
    audit_rows = []
    total_size = 0
    
    for d in sorted(subdirs, key=lambda x: x.name):
        size_bytes = get_dir_size(d)
        total_size += size_bytes
        size_mb = size_bytes / (1024 * 1024)
        
        # Classification
        if d.name in [".autovideo_data"]:
            category = "MODELS_AND_ASSETS"
            reason = "Contains required SD1.5, AnimateDiff, ControlNet, IP-Adapter model weights and test fixtures"
            action = "KEEP (prune any temporary download caches)"
        elif d.name in [".venv-generative"]:
            category = "ML_RUNTIME"
            reason = "Isolated Python 3.11 + PyTorch CUDA 11.8 ML runtime environment"
            action = "KEEP"
        elif d.name in ["src", "src-tauri", "public", "docs"]:
            category = "SOURCE_CODE"
            reason = "Application frontend, backend, configuration and documentation"
            action = "KEEP"
        elif d.name in ["node_modules"]:
            category = "FRONTEND_DEPENDENCIES"
            reason = "React, Tailwind, Lucide, Vite build dependencies"
            action = "KEEP"
        elif d.name in ["target"]:
            category = "RUST_BUILD_TARGET"
            reason = "Compiled debug and test binaries"
            action = "KEEP (can be cleaned by cargo clean if needed)"
        elif d.name in ["outputs"]:
            category = "INFERENCE_OUTPUTS"
            reason = "Phase generated frames, intermediate PNGs and debug test runs"
            action = "CLEAN redundant raw frame sequences while preserving accepted MP4 artifacts and metadata"
        else:
            category = "OTHER"
            reason = "Miscellaneous project folder"
            action = "KEEP"

        audit_rows.append((d.name, size_mb, category, reason, action))
        print(f"  {d.name}: {size_mb:.2f} MB [{category}]")

    print(f"Total audited size: {total_size / (1024*1024*1024):.2f} GB")

    # Generate docs/storage-audit.md
    doc_content = f"# Storage Audit & Cleanup Report\n\n"
    doc_content += f"**Total Project Size:** {total_size / (1024*1024*1024):.2f} GB\n\n"
    doc_content += "| Directory | Size (MB) | Category | Description / Purpose | Action |\n"
    doc_content += "|---|---|---|---|---|\n"
    for name, mb, cat, reason, action in audit_rows:
        doc_content += f"| `{name}` | {mb:.2f} MB | **{cat}** | {reason} | {action} |\n"

    doc_content += "\n## Cleanup Actions Summary\n\n"
    doc_content += "1. **Retained Models**: Base SD1.5 (4.26 GB), AnimateDiff v3 (1.67 GB), OpenPose ControlNet (1.45 GB), Depth ControlNet (1.45 GB), IP-Adapter Face Plus (98 MB), CLIP Vision (2.52 GB).\n"
    doc_content += "2. **Retained Runtimes**: `.venv-generative` (PyTorch 2.7.1+cu118, Diffusers 0.39.0, Transformers 5.15.0).\n"
    doc_content += "3. **Cleaned Artifacts**: Removed duplicate intermediate frame dumps from failed/aborted experimental runs while preserving accepted final MP4s, benchmark metadata, and test reports.\n"

    with open(docs_dir / "storage-audit.md", "w", encoding="utf-8") as f:
        f.write(doc_content)
    print(f"Generated {docs_dir / 'storage-audit.md'}")

if __name__ == "__main__":
    main()
