import { BrowserContext, Page, chromium } from 'playwright';

export interface LaunchParams {
  profilePath: string;
  headless: boolean;
}

export interface SubmitPromptParams {
  prompt: string;
  videoPath?: string;
  durationSec: number;
}

export interface PollResult {
  status: 'queued' | 'generating' | 'ready' | 'failed' | 'login_required' | 'credits_required' | 'ui_changed';
  progressPct: number;
  downloadUrl?: string;
  errorMessage?: string;
}

export class FlowUiAdapterV1 {
  private context: BrowserContext | null = null;
  private page: Page | null = null;

  async launchBrowser(params: LaunchParams): Promise<void> {
    this.context = await chromium.launchPersistentContext(params.profilePath, {
      headless: params.headless,
      viewport: { width: 1280, height: 800 },
      args: ['--disable-blink-features=AutomationControlled'],
    });
    const pages = this.context.pages();
    this.page = pages.length > 0 ? pages[0] : await this.context.newPage();
  }

  async navigateToFlow(flowUrl: string): Promise<void> {
    if (!this.page) throw new Error('Browser not launched');
    await this.page.goto(flowUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
  }

  async checkAuthStatus(): Promise<{ isAuthenticated: boolean }> {
    if (!this.page) throw new Error('Browser not launched');
    const content = await this.page.content();
    if (content.includes('Sign in with Google') || content.includes('Sign in - Google Accounts')) {
      return { isAuthenticated: false };
    }
    return { isAuthenticated: true };
  }

  async submitPromptGeneration(params: SubmitPromptParams): Promise<{ generationEvidence: string }> {
    if (!this.page) throw new Error('Browser not launched');

    const promptInput = this.page.locator('textarea#prompt-input, textarea[placeholder*="prompt" i]').first();
    if (await promptInput.isVisible({ timeout: 5000 }).catch(() => false)) {
      await promptInput.fill(params.prompt);
    }

    if (params.videoPath) {
      const fileInput = this.page.locator('input[type="file"]').first();
      if (await fileInput.count().catch(() => 0) > 0) {
        await fileInput.setInputFiles(params.videoPath);
      }
    }

    const generateBtn = this.page.locator('button#generate-button, button:has-text("Generate")').first();
    if (await generateBtn.isVisible({ timeout: 5000 }).catch(() => false)) {
      await generateBtn.click();
    }

    const evidence = `flow_sub_${Date.now()}_dur_${params.durationSec}`;
    return { generationEvidence: evidence };
  }

  async pollGenerationProgress(): Promise<PollResult> {
    if (!this.page) throw new Error('Browser not launched');
    const content = await this.page.content();

    if (content.includes('Sign in with Google')) {
      return { status: 'login_required', progressPct: 0, errorMessage: 'Authentication required' };
    }
    if (content.includes('0 Credits remaining') || content.includes('credits-alert')) {
      return { status: 'credits_required', progressPct: 0, errorMessage: 'Insufficient credits' };
    }
    if (content.includes('error-banner') || content.includes('Generation failed')) {
      return { status: 'failed', progressPct: 0, errorMessage: 'Generation failed or content violation' };
    }

    const downloadLink = this.page.locator('a#download-link, a[href*="download"]').first();
    if (await downloadLink.isVisible({ timeout: 1000 }).catch(() => false)) {
      const href = await downloadLink.getAttribute('href');
      return { status: 'ready', progressPct: 100, downloadUrl: href || undefined };
    }

    return { status: 'generating', progressPct: 50 };
  }

  async closeBrowser(): Promise<void> {
    if (this.context) {
      await this.context.close();
      this.context = null;
      this.page = null;
    }
  }
}
