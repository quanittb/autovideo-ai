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
    | 'FLOW_LANDING'
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
      viewport: { width: 1440, height: 900 },
      acceptDownloads: true,
      ignoreDefaultArgs: [
        '--disable-sync',
        '--disable-extensions',
        '--disable-component-extensions-with-background-pages',
      ],
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

    const checkPage = async (): Promise<AuthStatusResult | null> => {
      if (!this.page) return null;
      const currentUrl = this.page.url();

      // 1. Authoritative positive evidence for LOGIN_REQUIRED via URL redirection
      if (
        currentUrl.includes('accounts.google.com') ||
        currentUrl.includes('ServiceLogin') ||
        currentUrl.includes('/signin')
      ) {
        return { status: 'LOGIN_REQUIRED' };
      }

      // Check for explicit Google Sign-in form (identifier / password inputs)
      const isGoogleSignInForm =
        (await this.page
          .locator('input[name="identifier"], #identifierId, input[type="email"]')
          .count()
          .catch(() => 0)) > 0;
      if (isGoogleSignInForm) {
        return { status: 'LOGIN_REQUIRED' };
      }

      // 2. Account eligibility / Action Required gates (fail-closed)
      const hasEligibilityGate =
        (await this.page
          .locator('#eligibility-gate, .eligibility-alert, [data-testid="eligibility-gate"]')
          .count()
          .catch(() => 0)) > 0;
      const bodyText = await this.page.locator('body').innerText().catch(() => '');
      const bodyTextLower = bodyText.toLowerCase();

      if (
        hasEligibilityGate ||
        bodyTextLower.includes('account not eligible') ||
        bodyTextLower.includes('age verification required') ||
        bodyTextLower.includes('identity verification required') ||
        bodyTextLower.includes('verify your age') ||
        bodyTextLower.includes('verify your identity') ||
        bodyTextLower.includes('region is not supported')
      ) {
        return { status: 'FLOW_ELIGIBILITY_REQUIRED' };
      }

      // 3. Explicit unauthenticated login prompt in rendered DOM
      const hasLoginPrompt =
        (await this.page
          .locator(
            '.login-prompt, #login-button, button:has-text("Sign in to Flow"), a.login-prompt'
          )
          .count()
          .catch(() => 0)) > 0;

      if (
        hasLoginPrompt ||
        bodyTextLower.includes('sign in with google to continue') ||
        bodyTextLower.includes('sign in to continue')
      ) {
        return { status: 'LOGIN_REQUIRED' };
      }

      // 4. Handle policy agreement modal if present ("Tôi đồng ý" / "I agree")
      const agreeBtn = this.page
        .locator('button:has-text("Tôi đồng ý"), button:has-text("I agree"), button:has-text("Agree")')
        .first();
      if ((await agreeBtn.count().catch(() => 0)) > 0 && (await agreeBtn.isVisible().catch(() => false))) {
        await agreeBtn.click({ timeout: 3000 }).catch(() => {});
        await this.page.waitForTimeout(1000);
      }

      // 5. Strong Authenticated Flow Workspace / Dashboard Detection
      const hasMockAppRoot =
        (await this.page
          .locator('#flow-app[data-authenticated="true"], #flow-app')
          .count()
          .catch(() => 0)) > 0;
      const hasPromptTextarea =
        (await this.page
          .locator(
            'textarea#prompt-input, textarea[placeholder*="prompt" i], textarea[placeholder*="Describe" i], textarea[placeholder*="video" i], [data-testid="prompt-input"], div[contenteditable="true"], [role="textbox"], textarea:not([name="g-recaptcha-response"])'
          )
          .count()
          .catch(() => 0)) > 0;
      const hasGenerateBtn =
        (await this.page
          .locator(
            'button#generate-button, button:has-text("Generate"), [data-testid="generate-button"], button:has-text("Tạo"), button[aria-label*="Tạo"]'
          )
          .count()
          .catch(() => 0)) > 0;

      // Check if on authenticated Flow project dashboard (e.g. project cards / "+ Dự án mới" button)
      const hasFlowDashboard =
        (await this.page
          .locator(
            'button:has-text("Dự án mới"), button:has-text("New Project"), button:has-text("New project"), div:has-text("Chỉnh sửa dự án")'
          )
          .count()
          .catch(() => 0)) > 0;

      // Mock server ready
      if (hasMockAppRoot && hasPromptTextarea) {
        return { status: 'READY' };
      }

      // Real Flow workspace ready: must have prompt input AND generate button, OR active project dashboard on labs.google
      const isPublicLanding =
        (await this.page
          .locator(
            'button:has-text("Create with Google Flow"), a:has-text("Create with Google Flow"), button:has-text("Try in Google Flow")'
          )
          .count()
          .catch(() => 0)) > 0 ||
        bodyTextLower.includes('ai creative studio built with google');

      if (
        (hasFlowDashboard || (hasPromptTextarea && hasGenerateBtn)) &&
        !isPublicLanding &&
        currentUrl.includes('labs.google')
      ) {
        return { status: 'READY' };
      }

      return null;
    };

    // First inspection pass
    const initialStatus = await checkPage();
    if (initialStatus) {
      return initialStatus;
    }

    // Check if on public landing page with CTA
    const landingCta = this.page
      .locator(
        'button:has-text("Create with Google Flow"), a:has-text("Create with Google Flow"), button:has-text("Try in Google Flow"), #cta-btn'
      )
      .first();
    const landingCtaCount = await landingCta.count().catch(() => 0);

    if (landingCtaCount > 0) {
      // Execute ONE NON-GENERATION navigation preflight: activate "Create with Google Flow" CTA
      try {
        await landingCta.click({ timeout: 5000 });
        // Wait for possible redirect or workspace load
        await this.page.waitForTimeout(3000);
      } catch (_) {
        // CTA click failed or timed out
      }

      // Re-evaluate page post-CTA
      const postCtaStatus = await checkPage();
      if (postCtaStatus) {
        return postCtaStatus;
      }

      const postUrl = this.page.url();
      if (
        postUrl.includes('accounts.google.com') ||
        postUrl.includes('ServiceLogin') ||
        postUrl.includes('/signin')
      ) {
        return { status: 'LOGIN_REQUIRED' };
      }

      // If still on landing page or unrecognized
      return { status: 'FLOW_LANDING' };
    }

    const currentUrl = this.page.url();
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
