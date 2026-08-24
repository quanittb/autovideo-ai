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
  status:
    | 'queued'
    | 'generating'
    | 'ready'
    | 'failed'
    | 'login_required'
    | 'credits_required'
    | 'ui_changed'
    | 'unknown';
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

/**
 * Shared Helper: Locates the active prompt composer input on the Flow workspace.
 * Supports textarea, contenteditable div, [role="textbox"].
 * Excludes recaptcha, hidden fields, and credential/login inputs.
 */
export async function locatePromptComposer(page: Page) {
  const selectorCandidates = [
    'div[data-slate-editor="true"]',
    'div[contenteditable="true"]',
    '[role="textbox"]',
    'textarea#prompt-input',
    'textarea[placeholder*="prompt" i]',
    'textarea[placeholder*="Describe" i]',
    'textarea[placeholder*="video" i]',
    'textarea[placeholder*="nhập" i]',
    'textarea[placeholder*="Mô tả" i]',
    '[data-testid="prompt-input"]',
    'textarea:not([name="g-recaptcha-response"])',
  ];

  for (const selector of selectorCandidates) {
    const loc = page.locator(selector);
    const count = await loc.count().catch(() => 0);
    for (let i = 0; i < count; i++) {
      const el = loc.nth(i);
      const isVisible = await el.isVisible().catch(() => false);
      if (isVisible) {
        const tagName = await el.evaluate((node: any) => node.tagName.toLowerCase()).catch(() => '');
        const isContentEditable = await el
          .evaluate(
            (node: any) =>
              node.isContentEditable ||
              node.getAttribute('contenteditable') === 'true' ||
              node.getAttribute('role') === 'textbox' ||
              node.hasAttribute('data-slate-editor')
          )
          .catch(() => false);
        const disabled = await el.isDisabled().catch(() => false);
        const ariaDisabled = ((await el.getAttribute('aria-disabled').catch(() => '')) || '').toLowerCase();

        if (disabled || ariaDisabled === 'true') continue;

        if (tagName === 'textarea' || tagName === 'input' || isContentEditable) {
          const name = ((await el.getAttribute('name').catch(() => '')) || '').toLowerCase();
          const type = ((await el.getAttribute('type').catch(() => '')) || '').toLowerCase();
          if (
            name.includes('recaptcha') ||
            name.includes('identifier') ||
            type === 'password' ||
            type === 'hidden'
          ) {
            continue;
          }
          return el;
        }
      }
    }
  }
  return null;
}

/**
 * Shared Helper: Locates the video file upload input.
 */
export async function locateUploadControl(page: Page) {
  const fileInput = page.locator('input[type="file"]').first();
  const exists = (await fileInput.count().catch(() => 0)) > 0;
  return exists ? fileInput : null;
}

/**
 * Shared Helper: Locates the authoritative Generate control in the active composer.
 * Strictly excludes generic navigation/new project buttons.
 */
export async function locateGenerateControl(page: Page) {
  const generateSelectors = [
    'button#generate-button',
    '[data-testid="generate-button"]',
    'button:has(i:has-text("arrow_forward"))',
    'button:has-text("arrow_forward")',
    'button:has-text("Generate")',
    'button:has-text("Tạo")',
    'button[aria-label*="Generate" i]',
    'button[aria-label*="Tạo" i]',
  ];

  for (const selector of generateSelectors) {
    const loc = page.locator(selector);
    const count = await loc.count().catch(() => 0);
    for (let i = 0; i < count; i++) {
      const btn = loc.nth(i);
      const isVisible = await btn.isVisible().catch(() => false);
      if (!isVisible) continue;

      const btnText = ((await btn.innerText().catch(() => '')) || '').trim().toLowerCase();
      const ariaLabel = ((await btn.getAttribute('aria-label').catch(() => '')) || '').toLowerCase();
      const ariaHasPopup = ((await btn.getAttribute('aria-haspopup').catch(() => '')) || '').toLowerCase();

      // Reject menu/dialog trigger buttons (e.g. "add_2\nTạo")
      if (ariaHasPopup === 'dialog' || ariaHasPopup === 'menu') {
        continue;
      }

      // Explicitly reject unrelated navigation / project management buttons
      if (
        btnText.includes('dự án mới') ||
        btnText.includes('new project') ||
        btnText.includes('create with google flow') ||
        btnText.includes('try in google flow') ||
        btnText.includes('sign in') ||
        btnText.includes('đóng') ||
        btnText.includes('close') ||
        btnText.includes('explore tools') ||
        btnText.includes('chỉnh sửa dự án') ||
        btnText.includes('xoá dự án') ||
        btnText.includes('tác nhân')
      ) {
        continue;
      }

      if (
        btnText.includes('arrow_forward') ||
        btnText === 'generate' ||
        btnText === 'tạo' ||
        btnText.startsWith('generate') ||
        btnText.startsWith('tạo') ||
        ariaLabel.includes('generate') ||
        ariaLabel.includes('tạo') ||
        selector.includes('#generate-button') ||
        selector.includes('[data-testid="generate-button"]')
      ) {
        return btn;
      }
    }
  }
  return null;
}

/**
 * Shared Helper: Authoritative semantic state detection. Fails closed on unrecognized DOM.
 * NEVER fabricates arbitrary 50% progress.
 */
export async function detectGenerationState(
  page: Page,
  _submissionEvidence?: string
): Promise<PollResult> {
  const currentUrl = page.url();

  // 1. Auth check
  if (
    currentUrl.includes('accounts.google.com') ||
    currentUrl.includes('ServiceLogin') ||
    currentUrl.includes('/signin')
  ) {
    return { status: 'login_required', progressPct: 0, errorMessage: 'Authentication expired' };
  }

  const isGoogleSignInForm =
    (await page
      .locator('input[name="identifier"], #identifierId, input[type="email"]')
      .count()
      .catch(() => 0)) > 0;
  if (isGoogleSignInForm) {
    return { status: 'login_required', progressPct: 0, errorMessage: 'Authentication required' };
  }

  const bodyText = await page.locator('body').innerText().catch(() => '');
  const bodyTextLower = bodyText.toLowerCase();

  if (
    bodyTextLower.includes('sign in with google to continue') ||
    bodyTextLower.includes('sign in to continue')
  ) {
    return { status: 'login_required', progressPct: 0, errorMessage: 'Authentication required' };
  }

  // 2. Eligibility check
  if (
    bodyTextLower.includes('account not eligible') ||
    bodyTextLower.includes('age verification required') ||
    bodyTextLower.includes('identity verification required') ||
    bodyTextLower.includes('region is not supported') ||
    (await page.locator('#eligibility-gate, [data-testid="eligibility-gate"]').count().catch(() => 0)) > 0
  ) {
    return {
      status: 'failed',
      progressPct: 0,
      errorMessage: 'FLOW_ELIGIBILITY_REQUIRED: Account or region verification required',
    };
  }

  // 3. Credits check
  if (
    bodyTextLower.includes('0 credits remaining') ||
    bodyTextLower.includes('0 tín dụng còn lại') ||
    (await page.locator('.credits-alert, #credits-alert, [data-testid="credits-alert"]').count().catch(() => 0)) > 0
  ) {
    return { status: 'credits_required', progressPct: 0, errorMessage: 'Insufficient Flow credits' };
  }

  // 4. Generation Error check
  const errorBanner = page.locator(
    '.error-banner, #error-banner, [data-testid="error-banner"], .generation-error'
  );
  if ((await errorBanner.count().catch(() => 0)) > 0 && (await errorBanner.first().isVisible().catch(() => false))) {
    const errMsg = await errorBanner.first().innerText().catch(() => 'Generation failed');
    return { status: 'failed', progressPct: 0, errorMessage: errMsg || 'Generation failed' };
  }
  if (bodyTextLower.includes('generation failed') || bodyTextLower.includes('quá trình tạo không thành công')) {
    return { status: 'failed', progressPct: 0, errorMessage: 'Flow generation failed' };
  }

  // 5. Ready / Download check
  const downloadLink = page
    .locator(
      'a#download-link, a[download], a[href*="download"], button:has-text("Download"), button:has-text("Tải xuống")'
    )
    .first();
  if ((await downloadLink.count().catch(() => 0)) > 0 && (await downloadLink.isVisible().catch(() => false))) {
    const href = await downloadLink.getAttribute('href').catch(() => null);
    if (href && href.trim().length > 0) {
      return {
        status: 'ready',
        progressPct: 100,
        downloadUrl: href.trim(),
      };
    } else {
      return {
        status: 'ready',
        progressPct: 100,
        downloadUrl: undefined,
      };
    }
  }

  const completedVideo = page
    .locator(
      'video[data-status="ready"], #flow-app [data-status="ready"], div[data-status="ready"]'
    )
    .first();
  if ((await completedVideo.count().catch(() => 0)) > 0 && (await completedVideo.isVisible().catch(() => false))) {
    return {
      status: 'ready',
      progressPct: 100,
    };
  }

  // 6. Generating / Queued check (Authoritative semantic progress markers)
  const progressIndicator = page
    .locator(
      '#progress-indicator, [data-testid="progress-indicator"], .progress-indicator, [data-status="generating"]'
    )
    .first();
  if ((await progressIndicator.count().catch(() => 0)) > 0 && (await progressIndicator.isVisible().catch(() => false))) {
    const progressAttr = await progressIndicator.getAttribute('data-progress').catch(() => null);
    const progressPct = progressAttr ? parseFloat(progressAttr) : 0;
    return { status: 'generating', progressPct: isNaN(progressPct) ? 0 : progressPct };
  }

  const generatingCard = page
    .locator(
      '.generating-card, [data-state="generating"], div:has-text("Đang tạo"), div:has-text("Generating...")'
    )
    .first();
  if ((await generatingCard.count().catch(() => 0)) > 0 && (await generatingCard.isVisible().catch(() => false))) {
    return { status: 'generating', progressPct: 0 };
  }

  // 7. Fail-closed: do NOT fabricate generating 50%
  if (
    currentUrl.includes('labs.google') ||
    currentUrl.includes('127.0.0.1') ||
    currentUrl.includes('localhost')
  ) {
    return {
      status: 'ui_changed',
      progressPct: 0,
      errorMessage: 'FLOW_UI_CHANGED: Unrecognized page state during generation polling',
    };
  }

  return {
    status: 'unknown',
    progressPct: 0,
    errorMessage: 'UNKNOWN: Could not determine Flow generation state',
  };
}

export class FlowUiAdapterV1 {
  public context: BrowserContext | null = null;
  public page: Page | null = null;

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
      if (
        errMsg.includes("Executable doesn't exist") ||
        errMsg.includes('Cannot find installed Chrome')
      ) {
        throw new Error(
          'CHROME_NOT_INSTALLED: Google Chrome Stable was not found. Please install Google Chrome to use Flow automation.'
        );
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
        .locator(
          'button:has-text("Tôi đồng ý"), button:has-text("I agree"), button:has-text("Agree")'
        )
        .first();
      if (
        (await agreeBtn.count().catch(() => 0)) > 0 &&
        (await agreeBtn.isVisible().catch(() => false))
      ) {
        await agreeBtn.click({ timeout: 3000 }).catch(() => {});
        await this.page.waitForTimeout(1000);
      }

      // 5. Strong Authenticated Flow Workspace / Dashboard Detection using shared helpers
      const hasMockAppRoot =
        (await this.page
          .locator('#flow-app[data-authenticated="true"], #flow-app')
          .count()
          .catch(() => 0)) > 0;

      const promptInput = await locatePromptComposer(this.page);
      const generateBtn = await locateGenerateControl(this.page);

      // Check if on authenticated Flow project dashboard (e.g. project cards / "+ Dự án mới" button)
      const hasFlowDashboard =
        (await this.page
          .locator(
            'button:has-text("Dự án mới"), button:has-text("New Project"), button:has-text("New project"), div:has-text("Chỉnh sửa dự án")'
          )
          .count()
          .catch(() => 0)) > 0;

      const isProjectWorkspace = currentUrl.includes('/tools/flow/project/');

      // Mock server ready
      if (hasMockAppRoot && promptInput) {
        return { status: 'READY' };
      }

      // Real Flow workspace ready: must have project workspace OR active dashboard OR prompt composer
      const isPublicLanding =
        (await this.page
          .locator(
            'button:has-text("Create with Google Flow"), a:has-text("Create with Google Flow"), button:has-text("Try in Google Flow")'
          )
          .count()
          .catch(() => 0)) > 0 ||
        bodyTextLower.includes('ai creative studio built with google');

      if (
        (isProjectWorkspace || hasFlowDashboard || (promptInput && generateBtn)) &&
        !isPublicLanding &&
        currentUrl.includes('labs.google')
      ) {
        return { status: 'READY' };
      }

      return null;
    };

    // If on non-localized landing URL, navigate directly to /fx/vi/tools/flow
    const initialUrl = this.page.url();
    if (initialUrl.endsWith('/fx/tools/flow') || initialUrl.includes('#models')) {
      await this.page.goto('https://labs.google/fx/vi/tools/flow', { waitUntil: 'domcontentloaded', timeout: 30000 }).catch(() => {});
      await this.page.waitForTimeout(3000);
    }

    // Bounded inspection loop allowing SPA hydration and redirects to settle
    for (let attempt = 0; attempt < 5; attempt++) {
      const status = await checkPage();
      if (status) {
        return status;
      }

      // Check if on public landing page with CTA
      const landingCta = this.page
        .locator(
          'button:has-text("Create with Google Flow"), a:has-text("Create with Google Flow"), button:has-text("Try in Google Flow")'
        )
        .first();
      const landingCtaCount = await landingCta.count().catch(() => 0);

      if (landingCtaCount > 0 && (await landingCta.isVisible().catch(() => false))) {
        try {
          await landingCta.click({ timeout: 5000 });
          await this.page.waitForTimeout(2000);
        } catch (_) {}

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
      }

      await this.page.waitForTimeout(500);
    }

    const currentUrl = this.page.url();
    if (
      currentUrl.includes('labs.google') ||
      currentUrl.includes('127.0.0.1') ||
      currentUrl.includes('localhost')
    ) {
      return { status: 'FLOW_UI_CHANGED' };
    }

    return { status: 'UNKNOWN' };
  }

  async dryRunPreflight(params: { prompt: string; videoPath?: string }): Promise<{
    authStatus: string;
    workspaceAccessible: boolean;
    promptLocated: boolean;
    uploadLocated: boolean;
    generateLocated: boolean;
    generateEnabled: boolean;
  }> {
    if (!this.page) throw new Error('Browser not launched');

    const auth = await this.checkAuthStatus();
    if (auth.status !== 'READY') {
      return {
        authStatus: auth.status,
        workspaceAccessible: false,
        promptLocated: false,
        uploadLocated: false,
        generateLocated: false,
        generateEnabled: false,
      };
    }

    // If on project dashboard, enter active project workspace (bounded check)
    for (let attempt = 0; attempt < 5; attempt++) {
      if (this.page.url().includes('/project/')) break;
      const projectLink = this.page.locator('a[href*="/tools/flow/project/"]').first();
      if ((await projectLink.count().catch(() => 0)) > 0 && (await projectLink.isVisible().catch(() => false))) {
        await projectLink.click().catch(() => {});
        await this.page.waitForTimeout(4000);
        break;
      }
      const newProjBtn = this.page.locator('button:has-text("Dự án mới"), button:has-text("New Project")').first();
      if ((await newProjBtn.count().catch(() => 0)) > 0 && (await newProjBtn.isVisible().catch(() => false))) {
        await newProjBtn.click().catch(() => {});
        await this.page.waitForTimeout(4000);
        break;
      }
      await this.page.waitForTimeout(1000);
    }

    let promptEl = null;
    for (let attempt = 0; attempt < 10; attempt++) {
      promptEl = await locatePromptComposer(this.page);
      if (promptEl) break;
      await this.page.waitForTimeout(1000);
    }

    if (promptEl && params.prompt) {
      const tagName = await promptEl.evaluate((el: any) => el.tagName.toLowerCase());
      if (tagName === 'textarea' || tagName === 'input') {
        await promptEl.fill(params.prompt);
      } else {
        await promptEl.click();
        await this.page.keyboard.press('Control+A');
        await this.page.keyboard.press('Backspace');
        await this.page.keyboard.type(params.prompt);
      }
      await this.page.waitForTimeout(1000);
    }

    let uploadLocated = false;
    if (params.videoPath) {
      const uploadEl = await locateUploadControl(this.page);
      uploadLocated = uploadEl !== null;
    }

    const generateEl = await locateGenerateControl(this.page);
    const generateLocated = generateEl !== null;
    let generateEnabled = false;
    if (generateEl) {
      const disabled = await generateEl.isDisabled().catch(() => false);
      const ariaDisabled = await generateEl.getAttribute('aria-disabled').catch(() => null);
      generateEnabled = !disabled && ariaDisabled !== 'true';
    }

    return {
      authStatus: auth.status,
      workspaceAccessible: true,
      promptLocated: promptEl !== null,
      uploadLocated,
      generateLocated,
      generateEnabled,
    };
  }

  async submitPromptGeneration(params: SubmitPromptParams): Promise<SubmitResult> {
    const page = this.page;
    if (!page) throw new Error('Browser not launched');

    console.error(`[submitPromptGeneration] Starting submit, initial url: ${page.url()}`);

    // Ensure authenticated and past any landing CTA / agreement gates
    const auth = await this.checkAuthStatus();
    console.error(`[submitPromptGeneration] Auth check status: ${auth.status}, url: ${page.url()}`);
    if (auth.status !== 'READY') {
      throw new Error(`FLOW_AUTH_ERROR: Flow authentication status is ${auth.status}`);
    }

    // If on project dashboard, enter active project workspace (bounded check)
    for (let attempt = 0; attempt < 8; attempt++) {
      if (page.url().includes('/project/')) {
        console.error(`[submitPromptGeneration] In project workspace: ${page.url()}`);
        break;
      }
      const projectLink = page.locator('a[href*="/tools/flow/project/"]').first();
      const pCount = await projectLink.count().catch(() => 0);
      const pVis = await projectLink.isVisible().catch(() => false);
      console.error(`[submitPromptGeneration] Attempt ${attempt}: projectLink count=${pCount}, visible=${pVis}`);
      if (pCount > 0 && pVis) {
        console.error('[submitPromptGeneration] Clicking projectLink...');
        await projectLink.click().catch(() => {});
        await page.waitForTimeout(4000);
        break;
      }
      const newProjBtn = page.locator('button:has-text("Dự án mới"), button:has-text("New Project")').first();
      const nVis = await newProjBtn.isVisible().catch(() => false);
      if (nVis) {
        console.error('[submitPromptGeneration] Clicking newProjBtn...');
        await newProjBtn.click().catch(() => {});
        await page.waitForTimeout(4000);
        break;
      }
      await page.waitForTimeout(1000);
    }

    // 1. Locate Prompt Composer via shared helper (bounded wait)
    let promptInput = null;
    for (let attempt = 0; attempt < 15; attempt++) {
      promptInput = await locatePromptComposer(page);
      if (promptInput) {
        console.error(`[submitPromptGeneration] Prompt composer located on attempt ${attempt}`);
        break;
      }
      await page.waitForTimeout(1000);
    }

    if (!promptInput) {
      console.error(`[submitPromptGeneration] Failed to locate prompt composer, current url: ${page.url()}`);
      throw new Error('FLOW_UI_CHANGED: Prompt composer input not found or not actionable');
    }

    console.error('[submitPromptGeneration] Entering prompt text...');
    const tagName = await promptInput.evaluate((el: any) => el.tagName.toLowerCase());
    if (tagName === 'textarea' || tagName === 'input') {
      await promptInput.fill(params.prompt);
    } else {
      await promptInput.click();
      await page.keyboard.press('Control+A');
      await page.keyboard.press('Backspace');
      await page.keyboard.type(params.prompt);
    }
    await page.waitForTimeout(1000);

    // 2. Locate File Input if Video Path is provided
    if (params.videoPath) {
      console.error(`[submitPromptGeneration] Uploading video: ${params.videoPath}`);
      if (!fs.existsSync(params.videoPath)) {
        throw new Error(`FILE_NOT_FOUND: Upload video does not exist at ${params.videoPath}`);
      }

      const fileInput = await locateUploadControl(page);
      if (!fileInput) {
        console.error('[submitPromptGeneration] File input element not found');
        throw new Error('FLOW_UI_CHANGED: Video file upload input element not found');
      }

      try {
        await fileInput.setInputFiles(params.videoPath);
        console.error('[submitPromptGeneration] Video file set successfully, waiting for upload...');
        await page.waitForTimeout(3000);
      } catch (err: any) {
        throw new Error(`UPLOAD_FAILED: Failed to set input file: ${err?.message || String(err)}`);
      }
    }

    // 3. Locate Generate Button via shared helper (bounded wait for enabled state)
    console.error('[submitPromptGeneration] Locating Generate button...');
    let generateBtn = null;
    let btnEnabled = false;
    for (let attempt = 0; attempt < 30; attempt++) {
      generateBtn = await locateGenerateControl(page);
      if (generateBtn) {
        const disabled = await generateBtn.isDisabled().catch(() => false);
        const ariaDisabled = await generateBtn.getAttribute('aria-disabled').catch(() => null);
        btnEnabled = !disabled && ariaDisabled !== 'true';
        if (btnEnabled) {
          console.error(`[submitPromptGeneration] Generate button enabled on attempt ${attempt}`);
          break;
        }
      }
      await page.waitForTimeout(500);
    }

    if (!generateBtn) {
      console.error('[submitPromptGeneration] Generate button not found');
      throw new Error('FLOW_UI_CHANGED: Generate button not found or not actionable');
    }

    if (!btnEnabled) {
      console.error('[submitPromptGeneration] Generate button is disabled');
      throw new Error('FLOW_UI_CHANGED: Generate button is disabled');
    }

    // 4. Perform the ONE paid click
    const submittedAt = new Date().toISOString();
    console.error(`[submitPromptGeneration] CLICKING GENERATE EXACTLY ONCE at ${submittedAt}...`);
    try {
      await generateBtn.click({ timeout: 10000 });
      console.error('[submitPromptGeneration] Generate button clicked!');
    } catch (err: any) {
      throw new Error(`CLICK_FAILED: Failed to execute Generate click: ${err?.message || String(err)}`);
    }

    // 5. Post-click Semantic Browser Evidence verification (Bounded observation)
    let postClickEvidence: string | null = null;
    let postClickState = 'AMBIGUOUS';

    console.error('[submitPromptGeneration] Observing post-click semantic transition...');
    for (let attempt = 0; attempt < 20; attempt++) {
      await page.waitForTimeout(1000);

      const state = await detectGenerationState(page, params.localSubmissionAttemptId);
      console.error(`[submitPromptGeneration] Post-click attempt ${attempt} state:`, state.status);
      if (state.status === 'generating') {
        postClickState = 'GENERATING_OBSERVED';
        postClickEvidence = `semantic:generating:${submittedAt}:${params.localSubmissionAttemptId}`;
        break;
      } else if (state.status === 'ready') {
        postClickState = 'READY_OBSERVED';
        postClickEvidence = `semantic:ready:${submittedAt}:${params.localSubmissionAttemptId}`;
        break;
      } else if (state.status === 'credits_required') {
        postClickState = 'CREDITS_REQUIRED_OBSERVED';
        postClickEvidence = `semantic:credits_required:${submittedAt}`;
        break;
      } else if (state.status === 'failed') {
        postClickState = 'FAILED_OBSERVED';
        postClickEvidence = `semantic:failed:${submittedAt}:${state.errorMessage || ''}`;
        break;
      }

      // Check if button changed to disabled or generating indicator (Click dispatched)
      const isStillEnabled = await generateBtn.isEnabled().catch(() => false);
      const isStillVisible = await generateBtn.isVisible().catch(() => false);
      if (!isStillEnabled || !isStillVisible) {
        postClickState = 'CLICK_DISPATCHED_OBSERVED';
        postClickEvidence = `semantic:btn_dispatched:${submittedAt}:${params.localSubmissionAttemptId}`;
        break;
      }
    }

    if (!postClickEvidence || postClickState === 'AMBIGUOUS') {
      throw new Error(
        `GENERATION_AMBIGUOUS: No positive post-submission UI transition observed for attempt ${params.localSubmissionAttemptId}`
      );
    }

    const localFingerprint = `fp_${params.localSubmissionAttemptId}_dur_${params.durationSec}`;

    return {
      generationEvidence: postClickEvidence,
      localSubmissionAttemptId: params.localSubmissionAttemptId,
      postClickState,
      submittedAt,
      fingerprint: localFingerprint,
    };
  }

  async pollGenerationProgress(submissionEvidence: string): Promise<PollResult> {
    if (!this.page) throw new Error('Browser not launched');
    return detectGenerationState(this.page, submissionEvidence);
  }

  async downloadArtifact(
    downloadUrl: string | undefined,
    destinationPath: string
  ): Promise<{ success: boolean; savedPath: string }> {
    if (!this.page) throw new Error('Browser not launched');

    const dir = path.dirname(destinationPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    // 1. Try browser download event by clicking download element on page
    const downloadLocators = [
      'a#download-link',
      'a[download]',
      'a[href*="download"]',
      'button:has-text("Download")',
      'button:has-text("Tải xuống")',
      'button[aria-label*="Download" i]',
      'button[aria-label*="Tải xuống" i]',
    ];

    for (const sel of downloadLocators) {
      const loc = this.page.locator(sel).first();
      if ((await loc.count().catch(() => 0)) > 0 && (await loc.isVisible().catch(() => false))) {
        try {
          const downloadPromise = this.page.waitForEvent('download', { timeout: 8000 });
          await loc.click();
          const download = await downloadPromise;
          await download.saveAs(destinationPath);
          return { success: true, savedPath: destinationPath };
        } catch (_) {}
      }
    }

    // 2. If downloadUrl is provided, validate origin and download via context request
    if (downloadUrl && downloadUrl.trim().length > 0) {
      const trimmedUrl = downloadUrl.trim();
      const currentUrl = this.page.url();

      let targetFullUrl: string;
      if (trimmedUrl.startsWith('http://') || trimmedUrl.startsWith('https://')) {
        const parsed = new URL(trimmedUrl);
        const host = parsed.hostname.toLowerCase();
        if (
          !host.endsWith('labs.google') &&
          !host.endsWith('googleusercontent.com') &&
          !host.endsWith('googleapis.com') &&
          host !== '127.0.0.1' &&
          host !== 'localhost'
        ) {
          throw new Error(`SECURITY_VIOLATION: Untrusted download URL origin: ${trimmedUrl}`);
        }
        targetFullUrl = trimmedUrl;
      } else {
        const base = new URL(currentUrl);
        targetFullUrl = new URL(trimmedUrl, base.origin).toString();
      }

      if (this.context) {
        const response = await this.context.request.get(targetFullUrl);
        if (response.ok()) {
          const buf = await response.body();
          fs.writeFileSync(destinationPath, buf);
          return { success: true, savedPath: destinationPath };
        }
      }
    }

    throw new Error(
      'DOWNLOAD_CONTROL_NOT_OBSERVED: No valid download control or accessible URL was observed on the completed result'
    );
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
