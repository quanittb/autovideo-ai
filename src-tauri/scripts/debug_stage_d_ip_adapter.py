import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image
from diffusers import StableDiffusionPipeline, DDIMScheduler, AutoencoderKL

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\stage_d")
    out_dir.mkdir(parents=True, exist_ok=True)
    char_ref_path = r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png"
    sd15_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\sd15\v1-5-pruned-emaonly.safetensors"
    ip_dir = r"D:\rustProject\autovideo-ai\.autovideo_data\models\ip_adapter"

    print("=== AutoVideo AI Root-Cause Debug: Stage D (SD1.5 + IP-Adapter) ===")
    
    # 1. Load standalone FP32 VAE
    vae = AutoencoderKL.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="vae", torch_dtype=torch.float32).to("cuda")

    # 2. Load Pipeline
    print("Loading StableDiffusionPipeline in float32...")
    pipe = StableDiffusionPipeline.from_pretrained(
        "runwayml/stable-diffusion-v1-5",
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

    # 3. Load IP-Adapter
    print("Loading IP-Adapter weights...")
    pipe.load_ip_adapter(
        "h94/IP-Adapter",
        subfolder="models",
        weight_name="ip-adapter_sd15.safetensors",
        image_encoder_folder="models/image_encoder"
    )
    pipe.set_ip_adapter_scale(0.6)

    pipe.enable_sequential_cpu_offload()

    # 4. Prepare character reference image
    char_img = Image.open(char_ref_path).convert("RGB")
    print(f"Character reference loaded: {char_img.size}")

    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    steps = 20
    width = 288
    height = 512

    print(f"Generating IP-Adapter conditioned image: {width}x{height}, steps={steps}...")
    t0_infer = time.perf_counter()

    result = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        ip_adapter_image=[char_img],
        width=width,
        height=height,
        num_inference_steps=steps,
        generator=generator,
        guidance_scale=7.5
    )
    t_infer_ms = (time.perf_counter() - t0_infer) * 1000.0

    img = result.images[0]
    out_p = out_dir / "stage_d_output.png"
    img.save(out_p)

    arr = np.array(img)
    print(f"Saved Stage D Output: {out_p} ({os.path.getsize(out_p)} bytes)")
    print(f"Stats -> Min: {arr.min()}, Max: {arr.max()}, Mean: {arr.mean():.2f}, Std: {arr.std():.2f}, R: {arr[:,:,0].mean():.2f}, G: {arr[:,:,1].mean():.2f}, B: {arr[:,:,2].mean():.2f}")
    print(f"Inference Latency: {t_infer_ms:.2f} ms")
    print("=== Stage D IP-Adapter SUCCESS ===")

if __name__ == "__main__":
    main()
