import { expect, test, type Page } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '../../..');
const serverUrl = 'http://127.0.0.1:19080';
const maintenanceToken = 'web-smoke-secret';

let server: ChildProcessWithoutNullStreams | null = null;
let tempDir = '';
let fixture: SmokeFixture;

interface SmokeFixture {
  username: string;
  password: string;
  invite_code: string;
  reminders: Array<{
    reminder_id: string;
    batch_id: string;
    title: string;
  }>;
}

test.beforeAll(async () => {
  tempDir = mkdtempSync(resolve(tmpdir(), 'quartermaster-web-smoke-'));
  server = spawn('cargo', ['run', '-p', 'qm-server'], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QM_BIND: '127.0.0.1:19080',
      QM_DATABASE_URL: `sqlite://${resolve(tempDir, 'smoke.db')}?mode=rwc`,
      QM_WEB_DIST_DIR: resolve(repoRoot, 'web/build'),
      QM_ANDROID_SMOKE_SEED_TRIGGER_SECRET: maintenanceToken,
      RUST_LOG: 'warn'
    }
  });

  server.stdout.on('data', (data) => process.stdout.write(`[qm-server] ${data}`));
  server.stderr.on('data', (data) => process.stderr.write(`[qm-server] ${data}`));

  await waitForHealth();
  fixture = await seedSmokeData();
});

test.afterAll(async () => {
  server?.kill('SIGTERM');
  rmSync(tempDir, { recursive: true, force: true });
});

test('supports inventory review reminders and stock cleanup actions', async ({ page }) => {
  await login(page);

  await expect(page.getByRole('heading', { name: 'Batches' })).toBeVisible();
  await expect(page.getByRole('button', { name: /Smoke Rice/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Smoke Beans/ })).toBeVisible();

  await page.getByRole('link', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Locations' })).toBeVisible();

  await page.getByTestId('location-name-input').fill('Smoke Shelf');
  await page.getByTestId('location-kind-select').selectOption('pantry');
  await page.getByTestId('location-create').click();
  await expect(page.getByTestId('location-row-Smoke Shelf')).toBeVisible();

  await page.getByTestId('location-name-input').fill('Smoke Empty');
  await page.getByTestId('location-kind-select').selectOption('pantry');
  await page.getByTestId('location-create').click();
  await expect(page.getByTestId('location-row-Smoke Empty')).toBeVisible();

  await page.getByTestId('location-move-down-Smoke Shelf').click();
  await expect(page.getByTestId('settings-location-list').locator('.location-row h3')).toHaveText([
    'Pantry',
    'Fridge',
    'Freezer',
    'Smoke Empty',
    'Smoke Shelf'
  ]);
  await page.getByTestId('location-move-up-Smoke Shelf').click();
  await expect(page.getByTestId('settings-location-list').locator('.location-row h3')).toHaveText([
    'Pantry',
    'Fridge',
    'Freezer',
    'Smoke Shelf',
    'Smoke Empty'
  ]);

  await page.getByTestId('location-edit-Smoke Shelf').click();
  await page.getByTestId('location-name-input').fill('Smoke Shelf Renamed');
  await page.getByTestId('location-save-edit').click();
  await expect(page.getByTestId('location-row-Smoke Shelf Renamed')).toBeVisible();

  await page.getByTestId('location-delete-Smoke Empty').click();
  await page.getByTestId('location-delete-confirm').click();
  await expect(page.getByTestId('location-row-Smoke Empty')).toHaveCount(0);

  await page.reload();
  await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
  await expect(page.getByTestId('location-row-Smoke Shelf Renamed')).toBeVisible();

  await page.getByRole('link', { name: 'Inventory' }).click();
  await expect(page.getByRole('heading', { name: 'Batches' })).toBeVisible();

  await page.getByRole('button', { name: 'Add stock' }).click();
  await page.getByLabel('Product name').fill('Smoke Oats');
  await page.getByLabel('Brand').fill('Web');
  await page.getByLabel('Product family').selectOption('mass');
  await page.getByLabel('Preferred unit').selectOption('kg');
  await page.getByRole('button', { name: 'Create product' }).click();
  await expect(page.getByRole('button', { name: /Smoke Oats Web/ })).toBeVisible();
  await page.getByLabel('Stock quantity').fill('2');
  await page.locator('.stock-create-form').getByLabel('Unit').selectOption('kg');
  await page
    .locator('.stock-create-form')
    .getByLabel('Location')
    .selectOption({ label: 'Smoke Shelf Renamed' });
  await page.locator('.stock-create-form').getByRole('button', { name: 'Add stock' }).click();
  await page
    .locator('.inventory-list')
    .getByRole('button', { name: /Smoke Oats/ })
    .click();
  await expect(page.getByTestId('detail-quantity')).toHaveText('2 kg');
  await expect(page.locator('.detail-region').getByText('Smoke Shelf Renamed')).toBeVisible();

  await page.getByRole('button', { name: 'Edit' }).click();
  await expect(page.locator('.stock-edit-form').getByLabel('Location')).toContainText(
    'Smoke Shelf Renamed'
  );
  await page.locator('.stock-edit-form').getByLabel('Stock quantity').fill('1.5');
  await page.locator('.stock-edit-form').getByLabel('Expiry date').fill('2026-05-01');
  await page.locator('.stock-edit-form').getByLabel('Opened date').fill('2026-04-20');
  await page.locator('.stock-edit-form').getByLabel('Note').fill('Breakfast shelf');
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByTestId('detail-quantity')).toHaveText('1.5 kg');
  await expect(
    page.locator('.detail-region').getByText('2026-05-01', { exact: true })
  ).toBeVisible();
  await expect(
    page.locator('.detail-region').getByText('2026-04-20', { exact: true })
  ).toBeVisible();
  await expect(page.locator('.detail-region').getByText('Breakfast shelf')).toBeVisible();
  await expect(page.getByText('adjust')).toBeVisible();

  await page.getByRole('link', { name: 'Settings' }).click();
  await page.getByTestId('location-delete-Smoke Shelf Renamed').click();
  await page.getByTestId('location-delete-confirm').click();
  await expect(
    page.getByText('This location still has active stock. Move, consume, or discard it first.')
  ).toBeVisible();
  await page.getByRole('link', { name: 'Inventory' }).click();

  const firstReminder = fixture.reminders[0];
  await page.getByRole('button', { name: 'Open' }).first().click();
  await expect(page.getByRole('heading', { name: /Smoke/ }).last()).toBeVisible();
  await expect(page.getByRole('heading', { name: 'History' })).toBeVisible();

  await page.getByRole('button', { name: 'Ack' }).first().click();
  await expect(page.getByText(firstReminder.title)).toHaveCount(0);

  await page.getByLabel('Consume quantity').fill('10');
  await page.getByRole('button', { name: 'Consume' }).click();
  await expect(page.getByTestId('detail-quantity')).toHaveText('490 g');

  await page.getByRole('button', { name: 'Discard' }).click();
  await expect(page.getByText('Depleted')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Restore' })).toBeVisible();

  await page.getByRole('button', { name: 'Restore' }).click();
  await expect(page.getByText('In stock')).toBeVisible();
});

test('renders the join browser fallback from the served app', async ({ page }) => {
  await page.goto(`/join?invite=${fixture.invite_code}&server=${encodeURIComponent(serverUrl)}`);
  await expect(page.getByRole('heading', { name: 'Join Quartermaster' })).toBeVisible();
  await expect(page.getByText(fixture.invite_code)).toBeVisible();
});

async function login(page: Page) {
  await page.goto('/');
  await page.getByLabel('Server URL').fill(serverUrl);
  await page.getByLabel('Username').fill(fixture.username);
  await page.getByLabel('Password').fill(fixture.password);
  await page.getByRole('button', { name: 'Log in' }).click();
}

async function waitForHealth() {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${serverUrl}/healthz`);
      if (response.ok) {
        return;
      }
    } catch {
      await new Promise((resolveWait) => setTimeout(resolveWait, 500));
    }
  }
  throw new Error('qm-server did not become healthy in time');
}

async function seedSmokeData(): Promise<SmokeFixture> {
  const response = await fetch(`${serverUrl}/internal/maintenance/seed-smoke`, {
    method: 'POST',
    headers: {
      accept: 'application/json',
      'x-qm-maintenance-token': maintenanceToken
    }
  });
  if (!response.ok) {
    throw new Error(`smoke fixture failed with HTTP ${response.status}: ${await response.text()}`);
  }
  return (await response.json()) as SmokeFixture;
}
