import os
import sys
import time
import json
import hashlib
from pathlib import Path
import torch
import numpy as np
from PIL import Image, ImageFilter
from diffusers import StableDiffusionControlNetPipeline, ControlNetModel, DDIMScheduler, AutoencoderKL

def compute_sha256(filepath):
    h = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while chunk := f.read(16 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    out_dir = Path(r"D:\rustProject\autovideo-ai\outputs\phase11\debug\stage_c")
    out_dir.mkdir(parents=True, exist_ok=True)
    cnet_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\controlnet\control_v11p_sd15_openpose.safetensors"
    sd15_path = r"D:\rustProject\autovideo-ai\.autovideo_data\models\sd15\v1-5-pruned-emaonly.safetensors"
    raw_frame_path = r"D:\rustProject\autovideo-ai\outputs\phase11\preprocessing\frames\frame_0001.png"

    print("=== AutoVideo AI Root-Cause Debug: Stage C (SD1.5 + Pose ControlNet) ===")
    
    # 1. Load standalone FP32 VAE
    vae = AutoencoderKL.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="vae", torch_dtype=torch.float32).to("cuda")

    # 2. Load ControlNet
    print("Loading ControlNet OpenPose in float32...")
    controlnet = ControlNetModel.from_single_file(
        cnet_path,
        torch_dtype=torch.float32
    )

    # 3. Load Pipeline
    print("Loading StableDiffusionControlNetPipeline...")
    pipe = StableDiffusionControlNetPipeline.from_single_file(
        sd15_path,
        controlnet=controlnet,
        vae=vae,
        torch_dtype=torch.float32,
        use_safetensors=True,
        load_safety_checker=False
    )
    pipe.safety_checker = None
    pipe.scheduler = DDIMScheduler.from_config(
        pipe.scheduler.config,
        clip_sample=False,
        timestep_spacing="linspace",
        steps_offset=1
    )
    pipe.enable_sequential_cpu_offload()
    pipe.enable_attention_slicing("max")

    # 4. Prepare conditioning pose image (288x512)
    src_img = Image.open(raw_frame_path).convert("RGB").resize((288, 512), Image.Resampling.LANCZOS)
    pose_img = src_img.convert("L").filter(ImageFilter.FIND_EDGES).convert("RGB")
    pose_img.save(out_dir / "input_pose_conditioning.png")
    print(f"Conditioning image prepared: shape={pose_img.size}")

    prompt = "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail"
    negative_prompt = "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo"
    seed = 42
    generator = torch.Generator("cuda").manual_seed(seed)
    steps = 20
    width = 288
    height = 512

    print(f"Generating ControlNet frame: {width}x{height}, steps={steps}...")
    t0_infer = time.perf_counter()

    result = pipe(
        prompt=prompt,
        negative_prompt=negative_prompt,
        image=pose_img,
        width=width,
        height=height,
        num_inference_steps=steps,
        generator=generator,
        controlnet_conditioning_scale=0.8,
        guidance_scale=7.5
    )
    t_infer_ms = (time.perf_counter() - t0_infer) * 1000.0

    img = result.images[0]
    out_p = out_dir / "stage_c_output.png"
    img.save(out_p)

    arr = np.array(img)
    print(f"Saved Stage C Output: {out_p} ({os.path.getsize(out_p)} bytes)")
    print(f"Stats -> Min: {arr.min()}, Max: {arr.max()}, Mean: {arr.mean():.2f}, Std: {arr.std():.2f}, R: {arr[:,:,0].mean():.2f}, G: {arr[:,:,1].mean():.2f}, B: {arr[:,:,2].mean():.2f}")
    print(f"Inference Latency: {t_infer_ms:.2f} ms")
    print("=== Stage C ControlNet SUCCESS ===")

if __name__ == "__main__":
    main()
