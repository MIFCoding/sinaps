import { test, expect } from '@playwright/test';
import { pull } from './gestures';

test('обновление плавно возвращается, не дублируется и восстанавливается после ошибки', async ({
  browser,
}) => {
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
  let requests = 0,
    release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/v1/patients/page*', async (route) => {
    requests++;
    await gate;
    await route.continue();
  });
  await page.evaluate(() => {
    const state = window as typeof window & { motionSamples: number[]; samplingMotion: boolean };
    state.motionSamples = [];
    state.samplingMotion = true;
    function sample() {
      if (!state.samplingMotion) return;
      const content = document.querySelector('.page-content');
      if (content && document.querySelector('.pull-indicator.returning')) {
        const transform = getComputedStyle(content).transform;
        state.motionSamples.push(transform === 'none' ? 0 : new DOMMatrixReadOnly(transform).m42);
      }
      requestAnimationFrame(sample);
    }
    requestAnimationFrame(sample);
  });
  try {
    await pull(page);
    await expect.poll(() => requests).toBe(1);
    await expect(page.getByRole('status').filter({ hasText: 'Обновляем' })).toBeVisible();
    await expect
      .poll(() =>
        page
          .locator('.page-content')
          .evaluate((node) => new DOMMatrixReadOnly(getComputedStyle(node).transform).m42),
      )
      .toBeCloseTo(32, 0);
    await pull(page);
    expect(requests).toBe(1);
    release();
    await expect(page.locator('.pull-indicator')).toHaveCount(0);
    const samples = await page.evaluate(() => {
      const state = window as typeof window & { motionSamples: number[]; samplingMotion: boolean };
      state.samplingMotion = false;
      return state.motionSamples;
    });
    expect(samples.filter((value) => value > 1 && value < 31).length).toBeGreaterThan(2);
    expect(
      Math.max(...samples.slice(1).map((value, index) => Math.abs(value - samples[index]))),
    ).toBeLessThan(26);
    await pull(page, 50);
    await expect(page.locator('.pull-indicator')).toHaveCount(0);
    expect(requests).toBe(1);
    await page.unroute('**/api/v1/patients/page*');

    // Ошибка запроса завершает анимацию и не мешает следующей попытке.
    await page.route('**/api/v1/patients/page*', (route) => route.abort(), { times: 1 });
    await pull(page);
    await expect(page.getByRole('alert')).toContainText('Не удалось связаться');
    await expect(page.locator('.pull-indicator')).toHaveCount(0);
    const retried = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === '/api/v1/patients/page' && response.status() === 200,
    );
    await pull(page);
    await retried;
    await expect(page.locator('.pull-indicator')).toHaveCount(0);
    await expect(page.getByRole('alert')).toHaveCount(0);
    // Смена экрана при завершении сессии не переносит индикатор в новый вход.
    await page.route(
      '**/api/v1/patients/page*',
      (route) =>
        route.fulfill({
          status: 401,
          contentType: 'application/json',
          body: JSON.stringify({ error: { code: 'unauthorized', message: 'Войдите в аккаунт' } }),
        }),
      { times: 1 },
    );
    await pull(page);
    await expect(
      page.getByRole('heading', { name: 'Добро пожаловать!', exact: true }),
    ).toBeVisible();
    await page.getByRole('button', { name: 'Войти', exact: true }).click();
    await expect(
      page.getByRole('heading', {
        name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
        exact: true,
      }),
    ).toBeVisible();
    await expect(page.locator('.pull-indicator')).toHaveCount(0);
    await expect(page.locator('.page-content')).not.toHaveClass(/is-pulling/);
    expect(errors).toEqual([]);
  } finally {
    release();
    await context.close();
  }
});

test('уменьшенное движение сохраняет обновление без длительного возврата', async ({ browser }) => {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    reducedMotion: 'reduce',
  });
  const page = await context.newPage();
  await page.goto('/?demo=1');
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    page.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  const response = page.waitForResponse(
    (result) =>
      new URL(result.url()).pathname === '/api/v1/patients/page' && result.status() === 200,
  );
  await pull(page);
  await response;
  await expect(page.locator('.pull-indicator')).toHaveCount(0);
  await expect(page.locator('.page-content')).not.toHaveClass(/is-pulling/);
  await context.close();
});
