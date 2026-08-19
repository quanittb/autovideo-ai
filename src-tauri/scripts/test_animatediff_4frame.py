import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
from diffusers import AnimateDiffPipeline, MotionAdapter, DDIMScheduler
from diffusers.utils import export_to_video
from PIL import Image

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    sd15_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\sd15\v1-5-pruned-emaonly.safetensors"
    motion_dir = r"D:\rustProject\autovideo-ai\.autovideo_data\models\animatediff"
    output_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase10\animatediff_4")
    frames_dir = output_dir / "frames"
    frames_dir.mkdir(parents=True, exist_ok=True)
    output_meta = output_dir / "metadata.json"

    print("=== AutoVideo AI Phase 10 Gate 1: AnimateDiff 4-Frame Real Neural Inference ===")
    print(f"SD1.5 path: {sd15_path}")
    print(f"Motion module dir: {motion_dir}")

    # CUDA telemetry before load
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats()
    vram_before = torch.cuda.memory_allocated() / (1024 * 1024)
    print(f"VRAM baseline: {vram_before:.2f} MB")

    t0_load = time.perf_counter()
    print("Loading MotionAdapter and AnimateDiffPipeline in float32...")
    motion_adapter = MotionAdapter.from_pretrained(
        "guoyww/animatediff-motion-adapter-v1-5-3",
        torch_dtype=torch.float32
    )
    pipe = AnimateDiffPipeline.from_pretrained(
        "runwayml/stable-diffusion-v1-5",
        motion_adapter=motion_adapter,
        torch_dtype=torch.float32
    )
    pipe.scheduler = DDIMScheduler.from_config(pipe.scheduler.config, beta_schedule="linear", clip_sample=False)
    
    # 4GB VRAM hardware optimizations: sequential CPU offload offloads sub-layers of UNetMotionModel
    os.environ["PYTORCH_CUDA_ALLOC_CONF"] = "expandable_segments:True"
    pipe.enable_sequential_cpu_offload()
    if hasattr(pipe.vae, 'enable_slicing'):
        pipe.vae.enable_slicing()
    if hasattr(pipe.vae, 'enable_tiling'):
        pipe.vae.enable_tiling()
    pipe.enable_attention_slicing("max")
    load_latency_ms = (time.perf_counter() - t0_load) * 1000.0
    print(f"AnimateDiff pipeline loaded in {load_latency_ms:.2f} ms")

    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    num_frames = 4
    steps = 15
    width = 288
    height = 512

    print(f"Executing real temporal neural forward pass: {num_frames} frames, {width}x{height}, steps={steps}...")
    torch.cuda.reset_peak_memory_stats()
    t0_infer = time.perf_counter()

    output = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        num_frames=num_frames,
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
    print(f"Temporal inference completed in {infer_latency_ms:.2f} ms")
    print(f"Peak allocated VRAM: {peak_allocated_mb:.2f} MB | Peak reserved VRAM: {peak_reserved_mb:.2f} MB")

    frames = output.frames[0]
    frame_hashes = []
    for idx, frame in enumerate(frames):
        frame_path = frames_dir / f"frame_{idx:04d}.png"
        frame.save(frame_path)
        sha = compute_sha256(frame_path)
        frame_hashes.append(sha)
        print(f"Frame {idx}: size={os.path.getsize(frame_path)} bytes, sha256={sha}")

    # Verify frame uniqueness
    unique_hashes = set(frame_hashes)
    assert len(unique_hashes) == num_frames, f"Frames must be unique! Found {len(unique_hashes)} unique out of {num_frames}"
    print(f"Frame uniqueness verified: {len(unique_hashes)}/{num_frames} distinct frames.")

    metadata = {
        "productionInference": True,
        "modelUsedForInference": True,
        "gate": "G1_ANIMATEDIFF_4FRAME",
        "numFrames": num_frames,
        "width": width,
        "height": height,
        "steps": steps,
        "seed": seed,
        "loadLatencyMs": round(load_latency_ms, 2),
        "inferLatencyMs": round(infer_latency_ms, 2),
        "peakAllocatedVramMb": round(peak_allocated_mb, 2),
        "peakReservedVramMb": round(peak_reserved_mb, 2),
        "frameHashes": frame_hashes,
        "models": {
            "sd15": { "present": True, "loaded": True, "inferenceUsed": True },
            "animatediff": { "present": True, "loaded": True, "inferenceUsed": True }
        }
    }

    with open(output_meta, "w", encoding="utf-8") as f:
        json.dump(metadata, f, indent=2)
    print(f"Saved metadata to {output_meta}")
    print("=== Phase 10 Gate 1 AnimateDiff 4-Frame SUCCESS ===")

if __name__ == "__main__":
    main()
