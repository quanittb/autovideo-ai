import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image
from diffusers import AutoencoderKL

def main():
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\stage_b")
    out_dir.mkdir(parents=True, exist_ok=True)

    print("=== Testing Standalone FP32 VAE Decoding of AnimateDiff Latents ===")
    
    # Load standalone FP32 VAE
    vae = AutoencoderKL.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="vae", torch_dtype=torch.float32).to("cuda")
    
    # Generate mock or real latents matching the AnimateDiff latents shape (1, 4, 4, 64, 36)
    # Let's run a test with random latents and inspect decode
    test_latents = torch.randn((1, 4, 4, 64, 36), dtype=torch.float32, device="cuda")
    
    # Permute to (B*F, C, H, W) = (4, 4, 64, 36)
    latents_permuted = test_latents.permute(0, 2, 1, 3, 4).reshape(-1, 4, 64, 36)
    print(f"Permuted shape for VAE: {latents_permuted.shape}")
    
    with torch.no_grad():
        decoded = vae.decode(latents_permuted / vae.config.scaling_factor).sample
    
    print(f"Decoded shape: {decoded.shape}")
    print(f"Decoded stats -> Min: {decoded.min().item():.4f}, Max: {decoded.max().item():.4f}, Mean: {decoded.mean().item():.4f}, Std: {decoded.std().item():.4f}")
    
    # Post-process [-1, 1] to [0, 255] uint8
    image = (decoded / 2 + 0.5).clamp(0, 1)
    image = image.cpu().permute(0, 2, 3, 1).float().numpy()
    image = (image * 255).round().astype("uint8")
    
    for idx in range(image.shape[0]):
        im = Image.fromarray(image[idx])
        p = out_dir / f"test_vae_decode_{idx}.png"
        im.save(p)
        print(f"Saved test decode {idx}: {p} ({os.path.getsize(p)} bytes)")

if __name__ == "__main__":
    main()
