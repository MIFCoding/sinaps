import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('история загружает старые записи, фильтрует на сервере и восстанавливается после ошибки', async ({
  browser,
}) => {
  test.setTimeout(120000);
  const doctor = await browser.newContext();
  const context = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const headers = { 'X-Synapse-Request': '1' };
  expect(
    (
      await doctor.request.post('/api/v1/auth/login', {
        headers,
        data: { email: 'doctor@synapse.demo', password: 'Synapse2026!' },
      })
    ).ok(),
  ).toBeTruthy();
  const id = randomUUID(),
    base = `/api/v1/patients/${id}`;
  expect(
    (
      await doctor.request.post('/api/v1/patients', {
        headers,
        data: {
          id,
          name: `История страниц ${id.slice(0, 8)}`,
          email: `history-e2e-${id}@synapse.demo`,
          birth_date: '1990-01-01',
        },
      })
    ).status(),
  ).toBe(201);
  await mkdir('tmp', { recursive: true });
  const previous = JSON.parse(
    await readFile('tmp/history-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  await writeFile('tmp/history-e2e-fixtures.json', JSON.stringify([...previous, id]));
  const invitation = await (await doctor.request.post(`${base}/invitation`, { headers })).json();
  expect(
    (
      await context.request.post('/api/v1/auth/activate', {
        headers,
        data: { code: invitation.code, password: 'История2026!' },
      })
    ).ok(),
  ).toBeTruthy();
  // Более 100 событий и 365 измерений воспроизводят прежние ограничения списка.
  const jobs: (() => Promise<void>)[] = [];
  for (let index = 0; index < 111; index++)
    jobs.push(async () => {
      expect(
        (
          await doctor.request.post(`${base}/events`, {
            headers,
            data: {
              id: randomUUID(),
              title: `Событие страниц ${index}`,
              body: 'Синтетическая запись',
              patient_visible: true,
              kind: index === 0 ? 'consultation' : 'note',
              occurred_at: '2020-01-01T12:00:00Z',
            },
          })
        ).status(),
      ).toBe(201);
    });
  for (let index = 0; index < 366; index++)
    jobs.push(async () => {
      expect(
        (
          await context.request.post(`${base}/diary`, {
            headers,
            data: {
              id: randomUUID(),
              measured_at: '2020-01-01T12:00:00Z',
              mood: 5,
              anxiety: 4,
              sleep_hours: 8,
              comment: `Дневник страниц ${index}`,
            },
          })
        ).status(),
      ).toBe(201);
    });
  for (let start = 0; start < jobs.length; start += 8)
    await Promise.all(jobs.slice(start, start + 8).map((job) => job()));
  expect(
    (
      await doctor.request.post(`${base}/events`, {
        headers,
        data: {
          id: randomUUID(),
          title: 'Скрытая запись страниц',
          body: 'Внутреннее содержание',
          patient_visible: false,
        },
      })
    ).status(),
  ).toBe(201);
  await page.goto('/?demo=1');
  await page.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await page.getByRole('button', { name: 'История наблюдения', exact: true }).click();
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(50);
  await page.getByRole('button', { name: 'Загрузить ещё события', exact: true }).click();
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(100);
  await page.getByRole('button', { name: 'Загрузить ещё события', exact: true }).click();
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(112);
  await expect(
    page.getByRole('button', { name: 'Загрузить ещё события', exact: true }),
  ).toHaveCount(0);
  expect(
    new Set(
      await page
        .locator('.event-timeline .timeline-item')
        .evaluateAll((items) => items.map((item) => item.id)),
    ).size,
  ).toBe(112);
  await expect(page.getByText('Скрытая запись страниц', { exact: true })).toHaveCount(0);
  await expect(page.locator('.diary-row')).toHaveCount(365);
  await page.getByRole('button', { name: 'Загрузить ещё записи дневника', exact: true }).click();
  await expect(page.locator('.diary-row')).toHaveCount(366);
  await expect(
    page.getByRole('button', { name: 'Загрузить ещё записи дневника', exact: true }),
  ).toHaveCount(0);
  await page.getByLabel('Фильтр по типу события').selectOption('consultation');
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(1);
  await expect(page.getByRole('heading', { name: 'Событие страниц 0', exact: true })).toBeVisible();
  // Ошибка первой страницы фильтра оставляет возможность повторить запрос.
  await page.route(
    `**${base}/events/page*`,
    (route) =>
      route.fulfill({ status: 503, json: { error: { message: 'Тестовая недоступность' } } }),
    { times: 1 },
  );
  await page.getByLabel('Фильтр по типу события').selectOption('note');
  await page.getByRole('button', { name: 'Повторить загрузку событий' }).click();
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(50);
  // Ответ предыдущего фильтра не должен заменить результат нового запроса.
  let release!: () => void, signal!: () => void;
  const blocked = new Promise<void>((resolve) => (release = resolve));
  const started = new Promise<void>((resolve) => (signal = resolve));
  await page.route(`**${base}/events/page*`, async (route) => {
    if (new URL(route.request().url()).searchParams.get('kind') === 'all') {
      const response = await route.fetch();
      signal();
      await blocked;
      await route.fulfill({ response });
    } else await route.continue();
  });
  await page.getByLabel('Фильтр по типу события').selectOption('all');
  await started;
  await page.getByLabel('Фильтр по типу события').selectOption('consultation');
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(1);
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await expect(page.locator('.event-timeline .timeline-item')).toHaveCount(1);
  await page
    .getByRole('navigation', { name: 'Мобильная навигация' })
    .getByRole('button', { name: 'Главная', exact: true })
    .click();
  await expect(page.locator('.latest-event')).toContainText('Начало наблюдения');
  await expect(page.locator('.latest-event')).not.toContainText('Событие страниц 0');
  expect(errors).toEqual([]);
  await context.close();
  await doctor.close();
});
