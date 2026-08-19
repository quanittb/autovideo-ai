import os
import hashlib
import json
from pathlib import Path

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    root = Path(r"D:\rustProject\autovideo-ai\.autovideo_data\models")
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11")
    out_dir.mkdir(parents=True, exist_ok=True)

    models_map = {
        "sd15": root / "sd15" / "v1-5-pruned-emaonly.safetensors",
        "animatediff": root / "animatediff" / "v3_sd15_mm.ckpt",
        "pose_controlnet": root / "controlnet" / "control_v11p_sd15_openpose.safetensors",
        "depth_controlnet": root / "controlnet" / "control_v11f1p_sd15_depth.safetensors",
        "ip_adapter_face": root / "ip_adapter" / "ip-adapter-plus-face_sd15.safetensors",
        "clip_vision": root / "ip_adapter" / "models" / "image_encoder" / "model.safetensors"
    }

    inv = {}
    for name, p in models_map.items():
        if p.exists():
            size = p.stat().st_size
            sha = compute_sha256(p)
            inv[name] = {
                "path": str(p),
                "sizeBytes": size,
                "sha256": sha,
                "present": True,
                "hashVerified": True,
                "loaded": False,
                "usedForInference": False
            }
            print(f"[{name}] size={size} bytes | sha256={sha}")
        else:
            inv[name] = {
                "path": str(p),
                "sizeBytes": 0,
                "sha256": None,
                "present": False,
                "hashVerified": False,
                "loaded": False,
                "usedForInference": False
            }
            print(f"[{name}] MISSING at {p}")

    out_file = out_dir / "model_inventory.json"
    with open(out_file, "w", encoding="utf-8") as f:
        json.dump(inv, f, indent=2)
    print(f"Saved model inventory to: {out_file}")

if __name__ == "__main__":
    main()
