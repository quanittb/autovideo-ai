import os
import shutil
from pathlib import Path

def main():
    root = Path(r"D:\rustProject\autovideo-ai")
    docs_dir = root / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)
    
    cleaned_items = []
    freed_bytes = 0

    # 1. Remove obsolete .ignored_node_modules
    ignored_nm = root / ".ignored_node_modules"
    if ignored_nm.exists():
        size = sum(f.stat().st_size for f in ignored_nm.rglob('*') if f.is_file())
        shutil.rmtree(ignored_nm, ignore_errors=True)
        freed_bytes += size
        cleaned_items.append((".ignored_node_modules", size / (1024*1024), "Obsolete duplicate node_modules directory"))

    # 2. Remove duplicate downloads in .autovideo_data/models
    dups = [
        root / ".autovideo_data" / "models" / "animatediff" / "diffusion_pytorch_model.safetensors",
        root / ".autovideo_data" / "models" / "controlnet" / "openpose",
        root / ".autovideo_data" / "models" / "controlnet" / "depth",
    ]
    for p in dups:
        if p.exists():
            if p.is_file():
                size = p.stat().st_size
                p.unlink()
            else:
                size = sum(f.stat().st_size for f in p.rglob('*') if f.is_file())
                shutil.rmtree(p, ignore_errors=True)
            freed_bytes += size
            cleaned_items.append((str(p.relative_to(root)), size / (1024*1024), "Redundant duplicate model weight copy"))

    print(f"Total freed space: {freed_bytes / (1024*1024*1024):.2f} GB")
    for name, mb, reason in cleaned_items:
        print(f"  Removed {name} ({mb:.2f} MB): {reason}")

    # Update docs/storage-audit.md
    with open(docs_dir / "storage-audit.md", "a", encoding="utf-8") as f:
        f.write("\n\n### Executed Cleanup Log\n\n")
        f.write(f"**Total Space Reclaimed:** {freed_bytes / (1024*1024*1024):.2f} GB\n\n")
        f.write("| Cleaned Path | Size (MB) | Rationale |\n")
        f.write("|---|---|---|\n")
        for name, mb, reason in cleaned_items:
            f.write(f"| `{name}` | {mb:.2f} MB | {reason} |\n")

if __name__ == "__main__":
    main()
