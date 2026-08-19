# AutoVideo AI — Cloud Provider Selection (MVP)

## 1. Selected Provider

- **Provider**: **Replicate**
- **Default Video Model**: `minimax/video-01` (Alternative / Fast Testing Model: `stability-ai/stable-video-diffusion:3f0457e4619da209757780f68b683fc95bced82a46f1ec0ba01321813ac2b8d1` or `lucataco/animate-diff:beecf6c963c663e032243973446b080516cb76449d0f584f4b9f35f2d43740dc`)
- **API Endpoint**: `https://api.replicate.com/v1/predictions`

## 2. Selection Rationale

1. **Standard REST & Streaming Polling API**:
   - Clean, well-documented HTTP endpoints (`POST /predictions`, `GET /predictions/{id}`, `POST /predictions/{id}/cancel`).
   - Secure server-side bearer token authentication via `REPLICATE_API_TOKEN`.
2. **Direct Video Artifact URLs**:
   - Returns standard direct HTTPS download links to MP4 output streams.
3. **Broad Model Portfolio**:
   - Host for state-of-the-art video-to-video, image-to-video, and character reference conditioning models.
4. **Transparent Pricing**:
   - Billed per second of compute execution or per prediction (typically $0.05 to $0.20 per short 4–6s generation).
5. **Cancellation & Timeout Support**:
   - Native cancellation endpoints prevent runaway billing on user abort.

## 3. Input & Output Contract

- **Inputs**:
  - `prompt`: Text transformation instruction.
  - `image` / `first_frame_image`: Base64 URI or public HTTPS URL to input character/scene frame.
  - `fps`: Output frame rate (default: 24–30).
  - `num_frames`: Number of generated frames (default: 16–32 for short MVP clips).
- **Outputs**:
  - Direct HTTPS URL to downloadable H.264 MP4 container.

## 4. Cost Model

- **Pricing Structure**: Predictable compute-based billing ($0.00115/sec on Nvidia A100 / fixed ~$0.08 per 5s video).
- **Cost Estimation**: Calculated deterministically based on target duration and resolution.
- **Status Reporting**:
  - If token present & pricing configured: `status = Estimated` / `Actual`.
  - If credentials absent: `status = Unknown` (Zero-Fake Guarantee).
