import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image
from diffusers import StableDiffusionPipeline

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    sd15_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\sd15\v1-5-pruned-emaonly.safetensors"
    out_path = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\baseline_sd15.png")
    out_meta = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\baseline_sd15_metadata.json")

    print("=== AutoVideo AI Root-Cause Debug: Stage A (SD1.5 Baseline) ===")
    print(f"Model path: {sd15_path}")
    
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats()
    t0_load = time.perf_counter()

    pipe = StableDiffusionPipeline.from_single_file(
        sd15_path,
        torch_dtype=torch.float32,
        use_safetensors=True,
        load_safety_checker=False
    )
    pipe.safety_checker = None
    pipe.enable_model_cpu_offload()
    if hasattr(pipe.vae, 'enable_slicing'):
        pipe.vae.enable_slicing()
    if hasattr(pipe.vae, 'enable_tiling'):
        pipe.vae.enable_tiling()
    pipe.enable_attention_slicing("max")

    t_load_ms = (time.perf_counter() - t0_load) * 1000.0
    print(f"Pipeline loaded in {t_load_ms:.2f} ms")

    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    steps = 20
    width = 288
    height = 512

    print(f"Generating image: {width}x{height}, steps={steps}, seed={seed}...")
    t0_infer = time.perf_counter()

    # Get latents first to inspect latent distribution
    latent_out = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        width=width,
        height=height,
        num_inference_steps=steps,
        generator=generator,
        guidance_scale=7.5,
        output_type="latent"
    )
    latents = latent_out.images # (1, 4, 64, 36)
    
    l_np = latents.detach().cpu().to(torch.float32).numpy()
    print(f"Latents Shape: {l_np.shape}")
    print(f"Latents Stats -> Min: {l_np.min():.4f}, Max: {l_np.max():.4f}, Mean: {l_np.mean():.4f}, Std: {l_np.std():.4f}")
    print(f"Latents NaN Count: {np.isnan(l_np).sum()}, Inf Count: {np.isinf(l_np).sum()}")

    # Decode latents through VAE
    pipe.vae.to("cuda")
    with torch.no_grad():
        latents_scaled = latents.to(device="cuda", dtype=torch.float32) / pipe.vae.config.scaling_factor
        decoded = pipe.vae.decode(latents_scaled).sample
    
    d_np = decoded.detach().cpu().numpy()
    print(f"Decoded Tensor Shape: {d_np.shape}")
    print(f"Decoded Stats -> Min: {d_np.min():.4f}, Max: {d_np.max():.4f}, Mean: {d_np.mean():.4f}, Std: {d_np.std():.4f}")
    print(f"Decoded NaN Count: {np.isnan(d_np).sum()}, Inf Count: {np.isinf(d_np).sum()}")

    images = pipe.image_processor.postprocess(decoded, output_type="pil")
    img = images[0]
    t_infer_ms = (time.perf_counter() - t0_infer) * 1000.0

    img.save(out_path)
    img_arr = np.array(img)
    print(f"Saved Image: {out_path} ({os.path.getsize(out_path)} bytes)")
    print(f"Image Array Shape: {img_arr.shape}, Dtype: {img_arr.dtype}")
    print(f"Image Array Stats -> Min: {img_arr.min()}, Max: {img_arr.max()}, Mean: {img_arr.mean():.2f}, Std: {img_arr.std():.2f}")
    print(f"Channel Means -> R: {img_arr[:,:,0].mean():.2f}, G: {img_arr[:,:,1].mean():.2f}, B: {img_arr[:,:,2].mean():.2f}")

    meta = {
        "stage": "STAGE_A_SD15_BASELINE",
        "modelPath": sd15_path,
        "imagePath": str(out_path),
        "fileSizeBytes": os.path.getsize(out_path),
        "sha256": compute_sha256(out_path),
        "width": width,
        "height": height,
        "steps": steps,
        "seed": seed,
        "loadLatencyMs": round(t_load_ms, 2),
        "inferLatencyMs": round(t_infer_ms, 2),
        "peakAllocatedVramMb": round(torch.cuda.max_memory_allocated() / (1024 * 1024), 2),
        "latentStats": {
            "min": float(l_np.min()),
            "max": float(l_np.max()),
            "mean": float(l_np.mean()),
            "std": float(l_np.std()),
            "nanCount": int(np.isnan(l_np).sum()),
            "infCount": int(np.isinf(l_np).sum())
        },
        "decodedStats": {
            "min": float(d_np.min()),
            "max": float(d_np.max()),
            "mean": float(d_np.mean()),
            "std": float(d_np.std()),
            "nanCount": int(np.isnan(d_np).sum()),
            "infCount": int(np.isinf(d_np).sum())
        },
        "imageStats": {
            "min": int(img_arr.min()),
            "max": int(img_arr.max()),
            "mean": float(img_arr.mean()),
            "std": float(img_arr.std()),
            "rMean": float(img_arr[:,:,0].mean()),
            "gMean": float(img_arr[:,:,1].mean()),
            "bMean": float(img_arr[:,:,2].mean())
        }
    }
    with open(out_meta, "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2)

if __name__ == "__main__":
    main()
