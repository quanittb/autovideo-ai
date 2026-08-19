import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image
from diffusers import AnimateDiffPipeline, MotionAdapter, DDIMScheduler, EulerDiscreteScheduler

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    sd15_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\sd15\v1-5-pruned-emaonly.safetensors"
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\stage_b")
    out_dir.mkdir(parents=True, exist_ok=True)

    print("=== AutoVideo AI Root-Cause Debug: Stage B (SD1.5 + AnimateDiff) ===")
    
    torch.cuda.empty_cache()
    torch.cuda.reset_peak_memory_stats()
    
    # Load motion adapter
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
    
    # Check scheduler configurations
    print(f"Default scheduler config: {pipe.scheduler.config}")
    pipe.scheduler = DDIMScheduler.from_config(
        pipe.scheduler.config,
        beta_schedule="linear",
        clip_sample=False,
        timestep_spacing="linspace",
        steps_offset=1
    )
    
    pipe.enable_sequential_cpu_offload()
    if hasattr(pipe.vae, 'enable_slicing'):
        pipe.vae.enable_slicing()
    if hasattr(pipe.vae, 'enable_tiling'):
        pipe.vae.enable_tiling()
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

    # Generate latents
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
    latents = latent_out.frames if hasattr(latent_out, 'frames') else latent_out[0]
    if isinstance(latents, list):
        latents = torch.stack(latents)
    elif not isinstance(latents, torch.Tensor):
        latents = torch.from_numpy(latents)
        
    l_np = latents.detach().cpu().to(torch.float32).numpy()
    print(f"AnimateDiff Latents Shape: {l_np.shape}")
    print(f"Latents Stats -> Min: {l_np.min():.4f}, Max: {l_np.max():.4f}, Mean: {l_np.mean():.4f}, Std: {l_np.std():.4f}")
    print(f"Latents NaN Count: {np.isnan(l_np).sum()}, Inf Count: {np.isinf(l_np).sum()}")

    # Decode latents
    # AnimateDiff latents are (batch, channels, frames, height, width) = (1, 4, F, 64, 36)
    # Permute to (batch*frames, channels, height, width) for VAE decode
    pipe.vae.to("cuda")
    # In diffusers AnimateDiff, latents are (1, 4, F, 64, 36) -> permute to (1, F, 4, 64, 36) -> reshape (F, 4, 64, 36)
    if latents.ndim == 5:
        # (1, 4, F, 64, 36) -> (F, 4, 64, 36)
        latents_for_vae = latents.permute(0, 2, 1, 3, 4).reshape(-1, latents.shape[1], latents.shape[3], latents.shape[4])
    else:
        latents_for_vae = latents

    print(f"Latents for VAE Shape: {latents_for_vae.shape}")
    with torch.no_grad():
        latents_scaled = latents_for_vae.to(device="cuda", dtype=torch.float32) / pipe.vae.config.scaling_factor
        decoded = pipe.vae.decode(latents_scaled).sample
    
    d_np = decoded.detach().cpu().numpy()
    print(f"Decoded Tensor Shape: {d_np.shape}")
    print(f"Decoded Stats -> Min: {d_np.min():.4f}, Max: {d_np.max():.4f}, Mean: {d_np.mean():.4f}, Std: {d_np.std():.4f}")

    images = pipe.image_processor.postprocess(decoded, output_type="pil")
    
    for idx, img in enumerate(images):
        p = out_dir / f"frame_{idx:04d}.png"
        img.save(p)
        arr = np.array(img)
        print(f"Frame {idx}: Shape={arr.shape}, Min={arr.min()}, Max={arr.max()}, Mean={arr.mean():.2f}, Std={arr.std():.2f}, R={arr[:,:,0].mean():.2f}, G={arr[:,:,1].mean():.2f}, B={arr[:,:,2].mean():.2f}")

if __name__ == "__main__":
    main()
