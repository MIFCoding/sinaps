import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('врач читает более 100 изменений, повторяет ошибку и не получает историю другого выбранного пациента', async ({
  browser,
}) => {
  test.setTimeout(90000);
  const context = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const page = await context.newPage();
  const headers = { 'X-Synapse-Request': '1' };
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/?demo=1');
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    page.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  const ids: string[] = [];
  const previous = JSON.parse(
    await readFile('tmp/profile-changes-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  let profile: Record<string, any> = {};
  for (const suffix of ['Основной', 'Другой']) {
    const id = randomUUID();
    const response = await page.request.post('/api/v1/patients', {
      headers,
      data: {
        id,
        name: `Версии ${suffix} ${id.slice(0, 8)}`,
        email: `changes-e2e-${id}@synapse.demo`,
        birth_date: '1990-01-01',
      },
    });
    expect(response.status()).toBe(201);
    ids.push(id);
    await mkdir('tmp', { recursive: true });
    await writeFile('tmp/profile-changes-e2e-fixtures.json', JSON.stringify([...previous, ...ids]));
    if (suffix === 'Основной') profile = await response.json();
  }
  const [id, otherId] = ids,
    base = `/api/v1/patients/${id}`;
  const keys = [
    'version',
    'name',
    'birth_date',
    'observation_started',
    'status',
    'medical_summary',
    'allergies',
  ];
  for (let index = 0; index < 105; index++) {
    const body = Object.fromEntries(keys.map((key) => [key, profile[key]]));
    const response = await page.request.patch(base, {
      headers,
      data: { ...body, medical_summary: `Сведения для проверки версий ${index}` },
    });
    expect(response.ok()).toBeTruthy();
    profile = await response.json();
  }
  await page.reload();
  const nav = page.getByRole('navigation', { name: 'Мобильная навигация' });
  await nav.getByRole('button', { name: 'История', exact: true }).click();
  const selection = page.getByLabel('Выбрать пациента', { exact: true });
  await selection.selectOption(id);
  const history = page.locator('.profile-history'),
    rows = history.locator('.profile-change');
  await expect(rows).toHaveCount(20);
  await expect(rows.first()).toContainText('Версия 106');
  for (const count of [40, 60, 80, 100, 105]) {
    await history.getByRole('button', { name: 'Загрузить ещё изменения', exact: true }).click();
    await expect(rows).toHaveCount(count);
  }
  await expect(history.getByRole('button')).toHaveCount(0);
  expect(new Set(await rows.locator('summary strong').allTextContents()).size).toBe(105);
  await rows.last().locator('summary').click();
  await expect(rows.last()).toContainText('Версия 2');
  await expect(rows.last()).toContainText('Сведения для проверки версий 0');
  await page.screenshot({ path: 'test-results/profile-changes-mobile.png' });
  // Переключение пациента заново получает первую страницу.
  await selection.selectOption(otherId);
  await expect(rows).toHaveCount(0);
  await selection.selectOption(id);
  await expect(rows).toHaveCount(20);
  await page.route(
    `**${base}/changes/page*`,
    (route) =>
      route.fulfill({
        status: 503,
        json: { error: { message: 'Тестовая недоступность журнала' } },
      }),
    { times: 1 },
  );
  await history.getByRole('button', { name: 'Загрузить ещё изменения', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Тестовая недоступность журнала');
  await expect(rows).toHaveCount(20);
  await history.getByRole('button', { name: 'Загрузить ещё изменения', exact: true }).click();
  await expect(rows).toHaveCount(40);
  // Задержанный ответ страницы старого пациента не попадает в открытую карточку другого.
  let release!: () => void, signal!: () => void;
  const blocked = new Promise<void>((resolve) => (release = resolve));
  const started = new Promise<void>((resolve) => (signal = resolve));
  await page.route(`**${base}/changes/page*`, async (route) => {
    const response = await route.fetch();
    signal();
    await blocked;
    await route.fulfill({ response });
  });
  await history.getByRole('button', { name: 'Загрузить ещё изменения', exact: true }).click();
  await started;
  await selection.selectOption(otherId);
  await expect(rows).toHaveCount(0);
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await expect(rows).toHaveCount(0);
  await expect(history.getByRole('button')).toHaveCount(0);
  expect(errors).toEqual([]);
  await context.close();
});
