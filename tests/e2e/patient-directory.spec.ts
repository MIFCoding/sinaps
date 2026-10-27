import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { pull } from './gestures';

test('серверный список загружает все страницы и сохраняет выбор пациента вне первой страницы', async ({
  browser,
}) => {
  test.setTimeout(120000);
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
  });
  const page = await context.newPage();
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
  const prefix = `Я-Пагинация ${randomUUID().slice(0, 8)}`;
  const fixtures = Array.from({ length: 61 }, (_, index) => ({
    id: randomUUID(),
    name: `${prefix} ${String(index).padStart(3, '0')}`,
  }));
  await mkdir('tmp', { recursive: true });
  const previous = JSON.parse(
    await readFile('tmp/directory-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  await writeFile('tmp/directory-e2e-fixtures.json', JSON.stringify([...previous, ...fixtures]));
  for (let index = 0; index < fixtures.length; index += 5) {
    await Promise.all(
      fixtures.slice(index, index + 5).map(async (item) => {
        const response = await page.request.post('/api/v1/patients', {
          headers: { 'X-Synapse-Request': '1' },
          data: {
            ...item,
            email: `directory-e2e-${item.id}@synapse.demo`,
            birth_date: '1990-01-01',
          },
        });
        expect(response.status()).toBe(201);
      }),
    );
  }
  const target = fixtures.at(-1)!;
  for (const [item, status] of [
    [fixtures[0], 'archived'],
    [target, 'observation'],
  ] as const) {
    const profile = await (await page.request.get(`/api/v1/patients/${item.id}`)).json();
    expect(
      (
        await page.request.patch(`/api/v1/patients/${item.id}`, {
          headers: { 'X-Synapse-Request': '1' },
          data: {
            version: profile.version,
            name: profile.name,
            birth_date: profile.birth_date,
            observation_started: profile.observation_started,
            status,
            medical_summary: '',
            allergies: null,
          },
        })
      ).status(),
    ).toBe(200);
  }
  const initial = await (await page.request.get('/api/v1/patients/page?limit=50')).json();
  expect(initial.items.some((item: { id: string }) => item.id === target.id)).toBe(false);
  await page.reload();
  const nav = page.getByRole('navigation', { name: 'Мобильная навигация' });
  await nav.getByRole('button', { name: 'Пациенты', exact: true }).click();
  await page.getByLabel('Поиск пациента').fill(prefix);
  await expect(page.locator('.directory-result-count')).toHaveText('Найдено: 61');
  await expect(page.locator('.patient-list-card .person-row')).toHaveCount(20);
  for (const count of [40, 60, 61]) {
    await page.getByRole('button', { name: 'Показать ещё', exact: true }).click();
    await expect(page.locator('.patient-list-card .person-row')).toHaveCount(count);
  }
  await expect(page.getByRole('button', { name: 'Показать ещё', exact: true })).toHaveCount(0);
  const displayed = await page.locator('.patient-list-card .person-row strong').allTextContents();
  expect(new Set(displayed).size).toBe(61);
  await page.getByRole('button', { name: 'Архив', exact: true }).click();
  await expect(page.locator('.patient-list-card .person-row')).toHaveCount(1);
  await expect(page.locator('.patient-list-card')).toContainText(fixtures[0].name);
  await page.getByRole('button', { name: 'На наблюдении', exact: true }).click();
  await expect(page.locator('.patient-list-card .person-row')).toHaveCount(1);
  await expect(page.locator('.patient-list-card')).toContainText(target.name);
  await page.getByRole('button', { name: 'Все', exact: true }).click();
  await page.getByLabel('Поиск пациента').fill(target.name);
  await expect(page.locator('.patient-list-card .person-row')).toHaveCount(1);
  await page.getByRole('button', { name: new RegExp(target.name) }).click();
  await expect(page.getByLabel('Выбрать пациента', { exact: true })).toHaveValue(target.id);
  await page.evaluate(() => scrollTo(0, 0));
  await pull(page);
  await expect(page.locator('.pull-indicator')).toHaveCount(0);
  await expect(page.getByLabel('Выбрать пациента', { exact: true })).toHaveValue(target.id);
  await nav.getByRole('button', { name: 'Пациенты', exact: true }).click();

  // Запоздавший ответ широкого поиска не заменяет результат нового точного запроса.
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let delayed = false;
  await page.route('**/api/v1/patients/page?*', async (route) => {
    if (new URL(route.request().url()).searchParams.get('search') !== prefix) {
      await route.continue();
      return;
    }
    delayed = true;
    const response = await route.fetch();
    await gate;
    await route.fulfill({ response });
  });
  try {
    await page.getByLabel('Поиск пациента').fill(prefix);
    await expect.poll(() => delayed).toBe(true);
    await page.getByLabel('Поиск пациента').fill(target.name);
    await expect(page.locator('.directory-result-count')).toHaveText('Найдено: 1');
    release();
    await expect(page.locator('.patient-list-card .person-row')).toHaveCount(1);
    await expect(page.locator('.patient-list-card')).toContainText(target.name);
    await expect
      .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
      .toBe(true);
    await page.screenshot({ path: 'test-results/patient-directory-mobile.png' });
    expect(errors).toEqual([]);
  } finally {
    release();
    await context.close();
  }
});
