import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image
from diffusers import AnimateDiffPipeline, MotionAdapter, DDIMScheduler, EulerDiscreteScheduler, AutoencoderKL

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\stage_b_proper")
    out_dir.mkdir(parents=True, exist_ok=True)

    print("=== AutoVideo AI Root-Cause Debug: AnimateDiff with SD1.5 Scaled Linear Betas ===")
    
    # 1. Load standalone FP32 VAE
    vae = AutoencoderKL.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="vae", torch_dtype=torch.float32).to("cuda")

    # 2. Load motion adapter
    print("Loading MotionAdapter...")
    motion_adapter = MotionAdapter.from_pretrained(
        "guoyww/animatediff-motion-adapter-v1-5-3",
        torch_dtype=torch.float32
    )
    
    print("Loading AnimateDiffPipeline...")
    pipe = AnimateDiffPipeline.from_pretrained(
        "runwayml/stable-diffusion-v1-5",
        motion_adapter=motion_adapter,
        torch_dtype=torch.float32
    )
    
    # Use DDIMScheduler with default scaled_linear schedule from SD1.5!
    pipe.scheduler = DDIMScheduler.from_config(
        pipe.scheduler.config,
        clip_sample=False,
        timestep_spacing="linspace",
        steps_offset=1
    )
    print(f"Scheduler beta_schedule: {pipe.scheduler.config.get('beta_schedule')}")

    pipe.enable_sequential_cpu_offload()
    pipe.enable_attention_slicing("max")

    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    num_frames = 4
    steps = 20
    width = 288
    height = 512

    print(f"Generating AnimateDiff frames: {num_frames} frames, {width}x{height}, steps={steps}...")
    t0_infer = time.perf_counter()

    latent_out = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        num_frames=num_frames,
        width=width,
        height=height,
        num_inference_steps=steps,
        generator=generator,
        guidance_scale=7.5,
        output_type="latent"
    )
    t_infer_ms = (time.perf_counter() - t0_infer) * 1000.0

    latents = latent_out.frames if hasattr(latent_out, 'frames') else latent_out[0]
    if isinstance(latents, list):
        latents = torch.stack(latents)
    elif not isinstance(latents, torch.Tensor):
        latents = torch.from_numpy(latents)

    # Permute from (1, 4, F, 64, 36) -> (F, 4, 64, 36)
    if latents.ndim == 5:
        latents_for_vae = latents.permute(0, 2, 1, 3, 4).reshape(-1, latents.shape[1], latents.shape[3], latents.shape[4])
    else:
        latents_for_vae = latents

    print(f"Latents Shape for VAE: {latents_for_vae.shape}")
    latents_f32 = latents_for_vae.to(device="cuda", dtype=torch.float32) / vae.config.scaling_factor
    with torch.no_grad():
        decoded = vae.decode(latents_f32).sample

    # Convert to PIL images
    image = (decoded / 2 + 0.5).clamp(0, 1)
    image = image.cpu().permute(0, 2, 3, 1).float().numpy()
    image = (image * 255).round().astype("uint8")

    for idx in range(image.shape[0]):
        im = Image.fromarray(image[idx])
        p = out_dir / f"frame_{idx:04d}.png"
        im.save(p)
        arr = np.array(im)
        print(f"Frame {idx}: Size={os.path.getsize(p)} bytes, Shape={arr.shape}, Min={arr.min()}, Max={arr.max()}, Mean={arr.mean():.2f}, Std={arr.std():.2f}, R={arr[:,:,0].mean():.2f}, G={arr[:,:,1].mean():.2f}, B={arr[:,:,2].mean():.2f}")

    print("=== Stage B Proper Scheduler SUCCESS ===")

if __name__ == "__main__":
    main()
