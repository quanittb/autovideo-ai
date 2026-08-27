const path = require('path');
const fs = require('fs');
const pwPath = path.resolve(__dirname, 'node_modules/playwright');
const { chromium } = require(pwPath);

async function runLoginHelper(profileId = 'profile_2') {
  const baseDir = path.resolve(__dirname, '../../.autovideo_data/flow_profiles', profileId);
  if (!fs.existsSync(baseDir)) {
    fs.mkdirSync(baseDir, { recursive: true });
  }

  console.log('================================================================');
  console.log(`🌐 ĐANG KHỞI CHẠY TRÌNH DUYỆT ĐĂNG NHẬP GOOGLE FLOW`);
  console.log(`📁 Profile: ${profileId}`);
  console.log(`👉 Cửa sổ trình duyệt đang mở lên...`);
  console.log(`👉 Bạn vui lòng đăng nhập tài khoản Google trên cửa sổ đó.`);
  console.log(`👉 Script sẽ tự động nhận diện khi đăng nhập thành công và lưu phiên.`);
  console.log('================================================================\n');

  const context = await chromium.launchPersistentContext(baseDir, {
    headless: false,
    viewport: { width: 1280, height: 800 },
    args: [
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-blink-features=AutomationControlled',
    ],
    ignoreDefaultArgs: ['--enable-automation'],
  });

  const page = context.pages()[0] || (await context.newPage());
  const targetUrl = 'https://labs.google/fx/vi/tools/flow';

  await page.goto(targetUrl, { waitUntil: 'domcontentloaded', timeout: 60000 });

  let authenticated = false;
  let attempts = 0;
  const maxAttempts = 600; // 10 minutes max wait

  while (!authenticated && attempts < maxAttempts) {
    attempts++;
    await new Promise((r) => setTimeout(r, 1000));

    try {
      const currentUrl = page.url();

      if (
        currentUrl.includes('accounts.google.com') ||
        currentUrl.includes('ServiceLogin') ||
        currentUrl.includes('/signin')
      ) {
        if (attempts % 5 === 0) {
          process.stdout.write('⏳ Đang đợi bạn nhập thông tin đăng nhập Google...\r');
        }
        continue;
      }

      // Check for Terms / Policy agreement modal ("Tôi đồng ý" / "I agree")
      const agreeBtn = page
        .locator('button:has-text("Tôi đồng ý"), button:has-text("I agree"), button:has-text("Agree")')
        .first();
      if ((await agreeBtn.count().catch(() => 0)) > 0 && (await agreeBtn.isVisible().catch(() => false))) {
        console.log('\n📝 Tự động xác nhận điều khoản dịch vụ Google Flow...');
        await agreeBtn.click().catch(() => {});
        await new Promise((r) => setTimeout(r, 1500));
      }

      const bodyText = await page.locator('body').innerText().catch(() => '');
      const bodyTextLower = bodyText.toLowerCase();

      // Check for positive indicators of authenticated Flow workspace
      const hasFlowDashboard =
        (await page
          .locator('button:has-text("Dự án mới"), button:has-text("New Project"), button:has-text("New project"), div:has-text("Chỉnh sửa dự án")')
          .count()
          .catch(() => 0)) > 0;

      const isProjectWorkspace = currentUrl.includes('/tools/flow/project/');
      const hasPromptInput =
        (await page
          .locator('textarea, div[contenteditable="true"], input[placeholder*="prompt"], [data-testid="prompt-input"]')
          .count()
          .catch(() => 0)) > 0;

      const isPublicLanding =
        (await page
          .locator('button:has-text("Create with Google Flow"), a:has-text("Create with Google Flow"), button:has-text("Try in Google Flow")')
          .count()
          .catch(() => 0)) > 0 ||
        bodyTextLower.includes('ai creative studio built with google');

      if ((isProjectWorkspace || hasFlowDashboard || hasPromptInput) && !isPublicLanding && currentUrl.includes('labs.google')) {
        authenticated = true;
        console.log('\n\n================================================================');
        console.log('🎉 XÁC THỰC THÀNH CÔNG! GOOGLE FLOW ĐÃ ĐĂNG NHẬP SẴN SÀNG (READY)!');
        console.log('💾 Đang lưu cookie và phiên làm việc vào Profile...');
        console.log('================================================================\n');

        // Update profile_meta.json
        const metaPath = path.join(baseDir, 'profile_meta.json');
        let meta = {
          profileId,
          name: `Flow Account ${profileId}`,
          profileDir: baseDir,
          isLocked: false,
          isAuthenticated: true,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        if (fs.existsSync(metaPath)) {
          try {
            const existing = JSON.parse(fs.readFileSync(metaPath, 'utf8'));
            meta = { ...existing, isAuthenticated: true, updatedAt: new Date().toISOString() };
          } catch (_) {}
        }
        fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), 'utf8');

        // Wait 3 seconds for Chromium to flush all storage to disk
        await new Promise((r) => setTimeout(r, 3000));
        await context.close();
        console.log('✅ Đã đóng trình duyệt an toàn. Bạn có thể quay lại app sử dụng ngay!');
        process.exit(0);
      }
    } catch (err) {
      // Ignore transient frame detachment during navigation
    }
  }

  if (!authenticated) {
    console.error('\n⚠️ Hết thời gian chờ đăng nhập (10 phút).');
    await context.close();
    process.exit(1);
  }
}

const targetProfile = process.argv[2] || 'profile_2';
runLoginHelper(targetProfile).catch((err) => {
  console.error('Lỗi khởi chạy:', err);
  process.exit(1);
});
