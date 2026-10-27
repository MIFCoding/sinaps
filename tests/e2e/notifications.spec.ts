import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { pull } from './gestures';

test('уведомления открывают записи, прочтение сохраняется, обновление работает жестом', async ({
  browser,
}) => {
  test.setTimeout(60000);
  const doctorContext = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
  });
  const patientContext = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
  });
  const doctor = await doctorContext.newPage();
  const patient = await patientContext.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (e) => errors.push(e.message));
  patient.on('pageerror', (e) => errors.push(e.message));
  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    doctor.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  await expect(doctor.getByRole('button', { name: 'Обновить данные', exact: true })).toHaveCount(0);
  const id = randomUUID(),
    name = `Пациент уведомлений ${id.slice(0, 8)}`;
  expect(
    (
      await doctor.request.post('/api/v1/patients', {
        headers: { 'X-Synapse-Request': '1' },
        data: { id, name, email: `notification-e2e-${id}@synapse.demo`, birth_date: '1990-01-01' },
      })
    ).status(),
  ).toBe(201);
  await mkdir('tmp', { recursive: true });
  const previous = JSON.parse(
    await readFile('tmp/notification-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  await writeFile('tmp/notification-e2e-fixtures.json', JSON.stringify([...previous, id]));
  const base = `/api/v1/patients/${id}`;
  const invitation = await (
    await doctor.request.post(`${base}/invitation`, { headers: { 'X-Synapse-Request': '1' } })
  ).json();
  expect(
    (
      await patient.request.post('/api/v1/auth/activate', {
        headers: { 'X-Synapse-Request': '1' },
        data: { code: invitation.code, password: 'ПарольУведомлений2026!' },
      })
    ).status(),
  ).toBe(200);
  await doctor.reload();
  const nav = doctor.getByRole('navigation', { name: 'Мобильная навигация' });
  await nav.getByRole('button', { name: 'Пациенты', exact: true }).click();
  await doctor.getByLabel('Поиск пациента').fill(name);
  await doctor.getByRole('button', { name: new RegExp(name) }).click();
  await expect(doctor.getByRole('heading', { name, exact: true })).toBeVisible();

  // Короткое движение, горизонтальный жест и прокрутка не должны отправлять запрос обновления.
  let refreshRequests = 0;
  doctor.on('request', (request) => {
    if (request.method() === 'GET' && new URL(request.url()).pathname === '/api/v1/patients/page')
      refreshRequests++;
  });
  await doctor.evaluate(() => scrollTo(0, 0));
  await pull(doctor, 50);
  await pull(doctor, 30, 100);
  await doctor.evaluate(() => scrollTo(0, 100));
  await pull(doctor);
  expect(refreshRequests).toBe(0);
  await doctor.evaluate(() => scrollTo(0, 0));
  const entry = {
    id: randomUUID(),
    measured_at: new Date().toISOString(),
    mood: 7,
    anxiety: 3,
    sleep_hours: 8,
    comment: 'Запись для уведомления врача',
  };
  expect(
    (
      await patient.request.post(`${base}/diary`, {
        headers: { 'X-Synapse-Request': '1' },
        data: entry,
      })
    ).status(),
  ).toBe(201);
  await pull(doctor);
  await expect.poll(() => refreshRequests).toBe(1);
  await expect(doctor.getByRole('button', { name: /^Уведомления, непрочитанных:/ })).toBeVisible();
  await doctor.getByRole('button', { name: /^Уведомления/ }).click();
  const dialog = doctor.getByRole('dialog', { name: 'Уведомления', exact: true });
  await expect(dialog.locator('.notification-item').filter({ hasText: name })).toHaveCount(1);
  await pull(doctor);
  expect(refreshRequests).toBe(1);
  // Сокращённая страница истории не должна мешать открыть запись по уведомлению.
  await doctor.route(`**${base}/diary/page*`, async (route) => {
    const response = await route.fetch();
    const entries = await response.json();
    await route.fulfill({
      response,
      json: {
        ...entries,
        items: entries.items.filter((item: { id: string }) => item.id !== entry.id),
      },
    });
  });
  const diaryDetail = doctor.waitForResponse(
    (response) => response.url().endsWith(`${base}/diary/${entry.id}`) && response.status() === 200,
  );
  await dialog.locator('.notification-item').filter({ hasText: name }).click();
  await diaryDetail;
  await expect(doctor.getByText(entry.comment, { exact: true })).toBeVisible();
  await doctor.unroute(`**${base}/diary/page*`);
  await expect(dialog).toHaveCount(0);
  const read = (await (await doctor.request.get('/api/v1/notifications')).json()).items.find(
    (item: { related_id: string }) => item.related_id === entry.id,
  );
  expect(read.read_at).not.toBeNull();

  const event = {
    id: randomUUID(),
    title: 'Опубликованная запись врача',
    body: 'Запись из уведомления',
    patient_visible: true,
    occurred_at: '2020-03-12T09:30:00+03:00',
  };
  expect(
    (
      await doctor.request.post(`${base}/events`, {
        headers: { 'X-Synapse-Request': '1' },
        data: event,
      })
    ).status(),
  ).toBe(201);
  await doctor.request.post(`${base}/events`, {
    headers: { 'X-Synapse-Request': '1' },
    data: { ...event, id: randomUUID(), title: 'Внутренняя запись', patient_visible: false },
  });
  await patient.route(`**${base}/events/page*`, async (route) => {
    const response = await route.fetch();
    const events = await response.json();
    await route.fulfill({
      response,
      json: {
        ...events,
        items: events.items.filter((item: { id: string }) => item.id !== event.id),
      },
    });
  });
  await patient.goto('/?demo=1');
  await patient.getByRole('button', { name: /^Уведомления/ }).click();
  await expect(patient.getByRole('dialog').locator('.notification-item')).toHaveCount(1);
  const eventDetail = patient.waitForResponse(
    (response) =>
      response.url().endsWith(`${base}/events/${event.id}`) && response.status() === 200,
  );
  await patient.getByRole('dialog').locator('.notification-item').click();
  await eventDetail;
  await expect(patient.getByRole('heading', { name: event.title, exact: true })).toBeVisible();
  await patient.unroute(`**${base}/events/page*`);
  await expect(patient.getByText('Внутренняя запись', { exact: true })).toHaveCount(0);
  await patient.reload();
  await expect(patient.getByRole('button', { name: 'Уведомления', exact: true })).toBeVisible();
  await expect(patient.getByRole('button', { name: /Уведомления, непрочитанных:/ })).toHaveCount(0);

  // Проверяем настоящую следующую страницу, а не скрытие части уже загруженного списка.
  for (let index = 0; index < 21; index++) {
    expect(
      (
        await doctor.request.post(`${base}/events`, {
          headers: { 'X-Synapse-Request': '1' },
          data: { ...event, id: randomUUID(), title: `Страница уведомлений ${index}` },
        })
      ).status(),
    ).toBe(201);
  }
  await patient.getByRole('button', { name: /^Уведомления/ }).click();
  await expect(patient.getByRole('dialog').locator('.notification-item')).toHaveCount(20);
  await patient.getByRole('button', { name: 'Показать ещё', exact: true }).click();
  await expect(patient.getByRole('dialog').locator('.notification-item')).toHaveCount(22);
  await patient.screenshot({ path: 'test-results/notifications-mobile.png' });
  await patient.getByRole('button', { name: 'Отметить все прочитанными', exact: true }).click();
  await expect(patient.getByRole('dialog').locator('.notification-item.unread')).toHaveCount(0);
  expect((await (await patient.request.get('/api/v1/notifications')).json()).unread_count).toBe(0);
  await patient.getByRole('button', { name: 'Закрыть уведомления', exact: true }).click();
  await expect
    .poll(() => patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  expect(errors).toEqual([]);
  await patientContext.close();
  await doctorContext.close();
});
