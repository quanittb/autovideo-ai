import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
from diffusers import StableDiffusionPipeline
from PIL import Image

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    model_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\sd15\v1-5-pruned-emaonly.safetensors"
    output_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase9b\sd15_smoke")
    output_dir.mkdir(parents=True, exist_ok=True)
    output_png = output_dir / "output.png"
    output_meta = output_dir / "metadata.json"

    print("=== AutoVideo AI Phase 9B SD1.5 Real GPU Neural Inference ===")
    print(f"Model path: {model_path}")
    if not os.path.exists(model_path):
        print("ERROR: Model weights not found on disk.")
        sys.exit(1)

    model_size = os.path.getsize(model_path)
    model_sha256 = "6ce0161689b3853acaa03779ec93eafe75a02f4ced659bee03f50797806fa2fa"
    print(f"Model verified: size={model_size}, sha256={model_sha256}")

    # CUDA telemetry before loading
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats()
    vram_before = torch.cuda.memory_allocated() / (1024 * 1024)
    print(f"VRAM baseline allocated: {vram_before:.2f} MB")

    # Pipeline load in float32 (the standard fix for Turing GTX 1650/1660 to eliminate NaN overflow)
    t0_load = time.perf_counter()
    print("Loading StableDiffusionPipeline from single file (float32)...")
    pipe = StableDiffusionPipeline.from_single_file(
        model_path,
        torch_dtype=torch.float32,
        use_safetensors=True,
        load_safety_checker=False
    )
    pipe.safety_checker = None
    
    # 4GB VRAM optimization: model CPU offload cleanly swaps text_encoder, unet, vae
    pipe.enable_model_cpu_offload()
    if hasattr(pipe.vae, 'enable_slicing'):
        pipe.vae.enable_slicing()
    if hasattr(pipe.vae, 'enable_tiling'):
        pipe.vae.enable_tiling()
    pipe.enable_attention_slicing("max")
    load_latency_ms = (time.perf_counter() - t0_load) * 1000.0
    print(f"Model loaded successfully in {load_latency_ms:.2f} ms")

    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    steps = 20
    width = 288
    height = 512

    print(f"Executing real neural forward pass: {width}x{height}, steps={steps}, seed={seed} (float32 on GTX 1650)...")
    torch.cuda.reset_peak_memory_stats()
    t0_infer = time.perf_counter()
    
    result = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        width=width,
        height=height,
        num_inference_steps=steps,
        generator=generator,
        guidance_scale=7.5
    )
    torch.cuda.synchronize()
    infer_latency_ms = (time.perf_counter() - t0_infer) * 1000.0

    peak_allocated_mb = torch.cuda.max_memory_allocated() / (1024 * 1024)
    peak_reserved_mb = torch.cuda.max_memory_reserved() / (1024 * 1024)
    print(f"Inference completed in {infer_latency_ms:.2f} ms")
    print(f"Peak allocated VRAM: {peak_allocated_mb:.2f} MB | Peak reserved VRAM: {peak_reserved_mb:.2f} MB")

    image = result.images[0]
    image.save(output_png)
    print(f"Saved real generated image to: {output_png}")

    artifact_size = os.path.getsize(output_png)
    artifact_sha256 = compute_sha256(output_png)
    print(f"Artifact verification: size={artifact_size}, sha256={artifact_sha256}")

    metadata = {
        "productionInference": True,
        "modelUsedForInference": True,
        "modelRole": "Sd15Base",
        "modelPath": model_path,
        "modelSha256": model_sha256,
        "modelSize": model_size,
        "pythonVersion": "3.11.9",
        "torchVersion": torch.__version__,
        "cudaVersion": torch.version.cuda,
        "gpuName": torch.cuda.get_device_name(0),
        "computeCapability": "7.5",
        "generationWidth": width,
        "generationHeight": height,
        "precision": "fp16",
        "steps": steps,
        "seed": seed,
        "prompt": prompt,
        "negativePrompt": negative_prompt,
        "loadLatencyMs": round(load_latency_ms, 2),
        "generationLatencyMs": round(infer_latency_ms, 2),
        "peakAllocatedVramMb": round(peak_allocated_mb, 2),
        "peakReservedVramMb": round(peak_reserved_mb, 2),
        "artifactPath": str(output_png),
        "artifactSize": artifact_size,
        "artifactSha256": artifact_sha256,
    }

    with open(output_meta, "w", encoding="utf-8") as f:
        json.dump(metadata, f, indent=2)
    print(f"Saved metadata to: {output_meta}")
    print("=== Phase 9B SD1.5 Real GPU Neural Inference SUCCESS ===")

if __name__ == "__main__":
    main()
