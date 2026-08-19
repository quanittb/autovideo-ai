# AutoVideo AI — Provider Architecture

## 1. Provider Abstraction Model

All AI operations are mediated through the decoupled `AiProvider` trait in `src-tauri/src/ai/hybrid/provider.rs`:

```rust
pub trait AiProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn config(&self) -> &ProviderConfig;
    fn health(&self) -> ProviderHealth;
    fn capability(&self) -> GenerationCapability;
    fn estimate_cost(&self, request: &GenerationRequest) -> CostEstimate;
    fn generate(&self, request: &GenerationRequest) -> Result<GenerationResult, GenerationError>;
}
```

## 2. Supported Provider Adapters

| Provider Adapter | Provider Type | Execution Target | Supported Conditioning | Health / Discovery |
|---|---|---|---|---|
| `LocalAiProvider` | `Local` | GPU / CPU PyTorch | ControlNet, OpenPose, Depth, IP-Adapter, AnimateDiff | Verified against local `.venv-generative` |
| `CloudImageProviderAdapter` | `CloudImage` | REST API (Keyframes) | IP-Adapter, ControlNet, Style Transfer | Checked via API Key configuration |
| `CloudVideoProviderAdapter` | `CloudVideo` | REST API (Video) | Direct video generation, motion guidance | Checked via API Key configuration |
| `ReplicateCloudProvider` | `CloudVideo` | Replicate HTTP API | Video-to-video diffusion models | Checked via `REPLICATE_API_TOKEN` environment variable |
| `MockAiProvider` | `Mock` | In-memory Contract | Architecture validation | Explicitly sets `is_mock = true`, `inference_used = false` |

## 3. Machine-Readable Error Classifications

The engine exposes structured error variants preventing silent failures or mock fallbacks:
- `PROVIDER_NOT_CONFIGURED`: Missing provider endpoints or configuration files.
- `PROVIDER_CREDENTIALS_MISSING`: API token or authentication secret absent.
- `PROVIDER_UNAVAILABLE`: Remote server unreachable or offline.
- `PROVIDER_RATE_LIMITED`: Provider HTTP 429 quota or rate limits hit.
- `BUDGET_EXCEEDED`: Estimated cloud cost exceeds user's hard budget ceiling.
- `CLOUD_COST_CONFIRMATION_REQUIRED`: Estimated cloud cost exceeds threshold, requiring user confirmation.
- `NO_CAPABLE_PROVIDER`: No healthy provider matches requested task capabilities.
- `NO_FEASIBLE_EXECUTION_PATH`: Local hardware is insufficient and no cloud provider is configured.
