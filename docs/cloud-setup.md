# AutoVideo AI — Cloud Setup & Cost Configuration

## 1. Environment Variable Configuration

AutoVideo AI securely discovers cloud credentials from system environment variables without storing plain text secrets in application manifests:

```bash
# Replicate API Integration
export REPLICATE_API_TOKEN="r8_xxxxxxxxxxxxxxxxxxxxxxxxx"

# Generic / Custom Cloud Video API
export AUTODEV_CLOUD_API_KEY="sk_live_xxxxxxxxxxxxxxxx"
export AUTODEV_CLOUD_ENDPOINT="https://api.autovideo-cloud.com/v1"
```

## 2. Cost Control & Confirmation Thresholds

In the application settings or `AIExecutionPreferences`, users can define budget safeguards:

- `maxCostUsd`: Hard cap on total spending per job (default: `$5.00`).
- `cloudCostConfirmationThreshold`: Jobs estimated above this value require an interactive confirmation prompt.
- `allowCloudFallback`: Allows automatic routing to cloud when local VRAM is insufficient.

## 3. Zero-Fake Pricing Policy

- If provider pricing is not configured in `ProviderConfig`, the cost estimator reports `status: UNKNOWN`.
- The user interface displays `"Estimated cost: UNKNOWN (Unconfigured Provider)"` rather than inventing a false price tag.
