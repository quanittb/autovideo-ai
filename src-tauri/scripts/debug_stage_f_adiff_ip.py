import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image
from diffusers import AnimateDiffPipeline, MotionAdapter, DDIMScheduler, AutoencoderKL

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\stage_f")
    out_dir.mkdir(parents=True, exist_ok=True)
    char_ref_path = r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png"

    print("=== AutoVideo AI Root-Cause Debug: Stage F (SD1.5 + AnimateDiff + IP-Adapter) ===")
    
    # 1. Load standalone FP32 VAE
    vae = AutoencoderKL.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="vae", torch_dtype=torch.float32).to("cuda")

    # 2. Load MotionAdapter
    print("Loading MotionAdapter...")
    motion_adapter = MotionAdapter.from_pretrained(
        "guoyww/animatediff-motion-adapter-v1-5-3",
        torch_dtype=torch.float32
    )

    # 3. Load AnimateDiffPipeline
    print("Loading AnimateDiffPipeline...")
    pipe = AnimateDiffPipeline.from_pretrained(
        "runwayml/stable-diffusion-v1-5",
        motion_adapter=motion_adapter,
        vae=vae,
        torch_dtype=torch.float32
    )
    pipe.safety_checker = None
    pipe.scheduler = DDIMScheduler.from_config(
        pipe.scheduler.config,
        clip_sample=False,
        timestep_spacing="linspace",
        steps_offset=1
    )

    # 4. Load IP-Adapter
    print("Loading IP-Adapter weights...")
    pipe.load_ip_adapter(
        "h94/IP-Adapter",
        subfolder="models",
        weight_name="ip-adapter_sd15.safetensors",
        image_encoder_folder="models/image_encoder"
    )
    pipe.set_ip_adapter_scale(0.6)
    pipe.enable_sequential_cpu_offload()

    # 5. Prepare character reference image
    char_img = Image.open(char_ref_path).convert("RGB")

    num_frames = 4
    width = 288
    height = 512
    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    steps = 20

    print(f"Generating AnimateDiff+IP-Adapter frames: {num_frames} frames, {width}x{height}, steps={steps}...")
    t0_infer = time.perf_counter()

    result = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        ip_adapter_image=[char_img],
        num_frames=num_frames,
        width=width,
        height=height,
        num_inference_steps=steps,
        generator=generator,
        guidance_scale=7.5
    )
    t_infer_ms = (time.perf_counter() - t0_infer) * 1000.0

    frames = result.frames[0]
    for idx, img in enumerate(frames):
        out_p = out_dir / f"frame_{idx:04d}.png"
        img.save(out_p)
        arr = np.array(img)
        print(f"Frame {idx}: Size={os.path.getsize(out_p)} bytes, Min={arr.min()}, Max={arr.max()}, Mean={arr.mean():.2f}, Std={arr.std():.2f}, R={arr[:,:,0].mean():.2f}, G={arr[:,:,1].mean():.2f}, B={arr[:,:,2].mean():.2f}")

    print(f"Stage F Latency: {t_infer_ms:.2f} ms")
    print("=== Stage F AnimateDiff + IP-Adapter SUCCESS ===")

if __name__ == "__main__":
    main()
