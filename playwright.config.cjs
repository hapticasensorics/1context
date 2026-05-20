module.exports = {
  // Playwright cleans outputDir at the start of a run. Keep that volatile
  // browser output away from repo-local test-results, which is our evidence
  // ledger for dogfood and release proofs.
  outputDir: '.playwright-artifacts/test-output',
  reporter: [
    ['list'],
    ['html', { outputFolder: '.playwright-artifacts/report', open: 'never' }]
  ],
  projects: [
    {
      name: 'chromium',
      use: { browserName: 'chromium' }
    }
  ],
  use: {
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    video: 'retain-on-failure'
  }
}
