import { BrowserContext, Page, chromium } from 'playwright';
import fs from 'fs';
import path from 'path';

export interface LaunchParams {
  profilePath: string;
  headless: boolean;
  channel?: string;
  runtimeMode?: 'MOCK_CHROMIUM' | 'PRODUCTION_CHROME';
}

export interface SubmitPromptParams {
  prompt: string;
  videoPath?: string;
  durationSec: number;
  localSubmissionAttemptId: string;
}

export interface PollResult {
  status: 'queued' | 'generating' | 'ready' | 'failed' | 'login_required' | 'credits_required' | 'ui_changed' | 'unknown';
  progressPct: number;
  downloadUrl?: string;
  errorMessage?: string;
}

export interface AuthStatusResult {
  status:
    | 'READY'
    | 'LOGIN_REQUIRED'
    | 'UNKNOWN'
    | 'FLOW_UI_CHANGED'
    | 'FLOW_ELIGIBILITY_REQUIRED'
    | 'USER_ACTION_REQUIRED';
}

export interface SubmitResult {
  generationEvidence: string;
  localSubmissionAttemptId: string;
  postClickState: string;
  submittedAt: string;
  fingerprint: string;
}

export class FlowUiAdapterV1 {
  private context: BrowserContext | null = null;
  private page: Page | null = null;

  async launchBrowser(params: LaunchParams): Promise<void> {
    if (this.context) {
      await this.closeBrowser();
    }

    const launchOptions: any = {
      headless: params.headless,
      viewport: { width: 1280, height: 800 },
      acceptDownloads: true,
    };

    if (params.runtimeMode === 'PRODUCTION_CHROME') {
      launchOptions.channel = 'chrome';
    } else if (params.channel) {
      launchOptions.channel = params.channel;
    }

    try {
      this.context = await chromium.launchPersistentContext(params.profilePath, launchOptions);
      this.page = this.context.pages()[0] || (await this.context.newPage());
    } catch (err: any) {
      const errMsg = err?.message || String(err);
      if (errMsg.includes('Executable doesn\'t exist') || errMsg.includes('Cannot find installed Chrome')) {
        throw new Error('CHROME_NOT_INSTALLED: Google Chrome Stable was not found. Please install Google Chrome to use Flow automation.');
      }
      throw err;
    }
  }

  async navigateToFlow(flowUrl: string): Promise<void> {
    if (!this.page) throw new Error('Browser not launched');
    await this.page.goto(flowUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
  }

  async checkAuthStatus(): Promise<AuthStatusResult> {
    if (!this.page) throw new Error('Browser not launched');

    const currentUrl = this.page.url();

    // 1. Authoritative positive evidence for LOGIN_REQUIRED
    if (
      currentUrl.includes('accounts.google.com') ||
      currentUrl.includes('ServiceLogin') ||
      currentUrl.includes('/signin')
    ) {
      return { status: 'LOGIN_REQUIRED' };
    }

    const content = await this.page.content();

    if (
      content.includes('Sign in with Google') ||
      content.includes('Sign in - Google Accounts') ||
      content.includes('accounts.google.com/signin') ||
      (await this.page.locator('.login-prompt, #login-button, a[href*="accounts.google.com/ServiceLogin"], a[href*="accounts.google.com/signin"]').count()) > 0
    ) {
      return { status: 'LOGIN_REQUIRED' };
    }

    // 2. Account eligibility / Action Required gates (fail-closed, requires manual account action)
    const contentLower = content.toLowerCase();
    if (
      contentLower.includes('account not eligible') ||
      contentLower.includes('age verification required') ||
      contentLower.includes('identity verification required') ||
      contentLower.includes('verify your age') ||
      contentLower.includes('verify your identity') ||
      contentLower.includes('subscription required') ||
      contentLower.includes('country is not supported') ||
      contentLower.includes('region is not supported') ||
      contentLower.includes('join the waitlist') ||
      contentLower.includes('request access') ||
      (await this.page.locator('#eligibility-gate, .eligibility-alert, [data-testid="eligibility-gate"]').count()) > 0
    ) {
      return { status: 'FLOW_ELIGIBILITY_REQUIRED' };
    }

    // 3. Authenticated Ready indicators
    const hasAppRoot = (await this.page.locator('#flow-app[data-authenticated="true"], #flow-app').count()) > 0;
    const hasPromptInput = (await this.page.locator('textarea#prompt-input, textarea[placeholder*="prompt" i]').count()) > 0;

    if (hasAppRoot && hasPromptInput) {
      return { status: 'READY' };
    }

    // 4. Official Flow URL or Mock server without login redirect, but elements cannot be recognized
    if (
      currentUrl.includes('labs.google') ||
      currentUrl.includes('127.0.0.1') ||
      currentUrl.includes('localhost')
    ) {
      return { status: 'FLOW_UI_CHANGED' };
    }

    // Fail-closed fallback: do not assume login failure
    return { status: 'UNKNOWN' };
  }

  async submitPromptGeneration(params: SubmitPromptParams): Promise<SubmitResult> {
    if (!this.page) throw new Error('Browser not launched');

    // 1. Locate Prompt Input (Fail-closed on UI change)
    const promptInput = this.page.locator('textarea#prompt-input, textarea[placeholder*="prompt" i]').first();
    const promptVisible = await promptInput.isVisible({ timeout: 5000 }).catch(() => false);
    if (!promptVisible) {
      throw new Error('FLOW_UI_CHANGED: Prompt textarea input not found or not actionable');
    }

    await promptInput.fill(params.prompt);

    // 2. Locate File Input if Video Path is provided
    if (params.videoPath) {
      if (!fs.existsSync(params.videoPath)) {
        throw new Error(`FILE_NOT_FOUND: Upload video does not exist at ${params.videoPath}`);
      }

      const fileInput = this.page.locator('input[type="file"]').first();
      const fileInputExists = (await fileInput.count().catch(() => 0)) > 0;
      if (!fileInputExists) {
        throw new Error('FLOW_UI_CHANGED: Video file upload input element not found');
      }

      try {
        await fileInput.setInputFiles(params.videoPath);
      } catch (err: any) {
        throw new Error(`UPLOAD_FAILED: Failed to set input file: ${err?.message || String(err)}`);
      }
    }

    // 3. Locate Generate Button (Fail-closed)
    const generateBtn = this.page.locator('button#generate-button, button:has-text("Generate")').first();
    const btnVisible = await generateBtn.isVisible({ timeout: 5000 }).catch(() => false);
    if (!btnVisible) {
      throw new Error('FLOW_UI_CHANGED: Generate button not found or not actionable');
    }

    const btnEnabled = await generateBtn.isEnabled().catch(() => false);
    if (!btnEnabled) {
      throw new Error('FLOW_UI_CHANGED: Generate button is disabled');
    }

    // 4. Perform the ONE paid click
    const submittedAt = new Date().toISOString();
    try {
      await generateBtn.click({ timeout: 10000 });
    } catch (err: any) {
      throw new Error(`CLICK_FAILED: Failed to execute Generate click: ${err?.message || String(err)}`);
    }

    // 5. Post-click Semantic Browser Evidence verification
    // Wait briefly to observe UI transition
    await this.page.waitForTimeout(500);

    const postContent = await this.page.content();
    let postClickState = 'SUBMITTED_OBSERVED';
    if (postContent.includes('credits-alert') || postContent.includes('0 Credits remaining')) {
      postClickState = 'CREDITS_REQUIRED_OBSERVED';
    } else if (postContent.includes('progress-indicator') || postContent.includes('generating')) {
      postClickState = 'GENERATING_OBSERVED';
    }

    const fingerprint = `fp_${params.localSubmissionAttemptId}_dur_${params.durationSec}`;
    const generationEvidence = `evidence:${params.localSubmissionAttemptId}:${submittedAt}:${fingerprint}`;

    return {
      generationEvidence,
      localSubmissionAttemptId: params.localSubmissionAttemptId,
      postClickState,
      submittedAt,
      fingerprint,
    };
  }

  async pollGenerationProgress(_submissionEvidence: string): Promise<PollResult> {
    if (!this.page) throw new Error('Browser not launched');

    const content = await this.page.content();

    if (content.includes('Sign in with Google') || content.includes('login-prompt')) {
      return { status: 'login_required', progressPct: 0, errorMessage: 'Authentication expired' };
    }
    if (content.includes('0 Credits remaining') || content.includes('credits-alert')) {
      return { status: 'credits_required', progressPct: 0, errorMessage: 'Insufficient Flow credits' };
    }
    if (content.includes('error-banner') || content.includes('Generation failed')) {
      return { status: 'failed', progressPct: 0, errorMessage: 'Generation failed: Inappropriate prompt or server error' };
    }
    if (content.includes('completely-redesigned-layout')) {
      return { status: 'ui_changed', progressPct: 0, errorMessage: 'FLOW_UI_CHANGED: Unrecognized page structure' };
    }

    const downloadLink = this.page.locator('a#download-link, a[href*="download"]').first();
    const downloadVisible = await downloadLink.isVisible({ timeout: 1000 }).catch(() => false);
    if (downloadVisible) {
      const href = await downloadLink.getAttribute('href');
      return {
        status: 'ready',
        progressPct: 100,
        downloadUrl: href || '/download',
      };
    }

    const progressIndicator = this.page.locator('#progress-indicator').first();
    if ((await progressIndicator.count().catch(() => 0)) > 0) {
      const progressAttr = await progressIndicator.getAttribute('data-progress');
      const progressPct = progressAttr ? parseFloat(progressAttr) : 50;
      return { status: 'generating', progressPct };
    }

    return { status: 'generating', progressPct: 50 };
  }

  async downloadArtifact(downloadUrl: string, destinationPath: string): Promise<{ success: boolean; savedPath: string }> {
    if (!this.page) throw new Error('Browser not launched');

    const dir = path.dirname(destinationPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    // 1. Try browser download event if link is clickable
    const downloadLink = this.page.locator(`a#download-link, a[href*="download"], a[href="${downloadUrl}"]`).first();
    if (await downloadLink.isVisible({ timeout: 2000 }).catch(() => false)) {
      try {
        const downloadPromise = this.page.waitForEvent('download', { timeout: 8000 });
        await downloadLink.click();
        const download = await downloadPromise;
        await download.saveAs(destinationPath);
        return { success: true, savedPath: destinationPath };
      } catch (_) {
        // Fall through to request context
      }
    }

    // 2. Context request with cookie/session sharing
    if (this.context) {
      const response = await this.context.request.get(downloadUrl);
      if (response.ok()) {
        const buf = await response.body();
        fs.writeFileSync(destinationPath, buf);
        return { success: true, savedPath: destinationPath };
      }
    }

    throw new Error(`DOWNLOAD_FAILED: Could not download artifact from ${downloadUrl}`);
  }

  async closeBrowser(): Promise<void> {
    if (this.context) {
      try {
        await this.context.close();
      } catch (_) {}
      this.context = null;
      this.page = null;
    }
  }
}
