import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('врач вносит прошлое событие с источником, пациент видит только опубликованную запись', async ({
  browser,
}) => {
  test.setTimeout(60000);
  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const patientContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const doctor = await doctorContext.newPage();
  const patient = await patientContext.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (error) => errors.push(error.message));
  patient.on('pageerror', (error) => errors.push(error.message));
  const id = randomUUID();
  const name = `Пациент истории ${id.slice(0, 8)}`;
  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    doctor.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  const created = await doctor.request.post('/api/v1/patients', {
    headers: { 'X-Synapse-Request': '1' },
    data: { id, name, email: `event-e2e-${id}@synapse.demo`, birth_date: '1990-01-01' },
  });
  expect(created.status()).toBe(201);
  await writeFile('test-results/event-fixtures.json', JSON.stringify([id]));
  await mkdir('tmp', { recursive: true });
  const previousFixtures = JSON.parse(
    await readFile('tmp/event-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  await writeFile('tmp/event-e2e-fixtures.json', JSON.stringify([...previousFixtures, id]));
  const invitation = await (
    await doctor.request.post(`/api/v1/patients/${id}/invitation`, {
      headers: { 'X-Synapse-Request': '1' },
    })
  ).json();
  const activated = await patient.request.post('/api/v1/auth/activate', {
    headers: { 'X-Synapse-Request': '1' },
    data: { code: invitation.code, password: 'ПарольИстории2026!' },
  });
  expect(activated.status()).toBe(200);
  await doctor.reload();
  const navigation = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await navigation.getByRole('button', { name: 'Пациенты' }).click();
  await doctor.getByLabel('Поиск пациента').fill(name);
  await doctor.getByRole('button', { name: new RegExp(name) }).click();
  await navigation.getByRole('button', { name: 'История', exact: true }).click();
  await doctor.getByRole('button', { name: 'Добавить заметку', exact: true }).click();
  await doctor.getByLabel('Тип события', { exact: true }).selectOption('consultation');
  await doctor.getByLabel('Дата и время события', { exact: true }).fill('2020-03-12T09:30');
  await doctor.getByLabel('Заголовок', { exact: true }).fill('Консультация в прошлом');
  await doctor
    .getByLabel('Запись врача', { exact: true })
    .fill('Синтетическая запись по документу');
  await doctor.getByLabel('Источник сведений', { exact: false }).fill('Выписка от 12.03.2020');
  await doctor.getByLabel('Показывать эту запись пациенту', { exact: true }).check();
  await doctor.getByRole('button', { name: 'Сохранить запись', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  const event = doctor
    .locator('.event-timeline .timeline-item')
    .filter({ has: doctor.getByRole('heading', { name: 'Консультация в прошлом', exact: true }) });
  await expect(event).toContainText('2020');
  await expect(event).toContainText('Выписка от 12.03.2020');
  await expect(event.locator('.event-recorded')).toContainText('Иван Сергеевич');
  const response = await doctor.request.get(`/api/v1/patients/${id}/events`);
  const records = await response.json();
  const stored = records.find(
    (entry: { title: string }) => entry.title === 'Консультация в прошлом',
  );
  expect(stored.kind).toBe('consultation');
  expect(new Date(stored.occurred_at).getTime()).toBe(new Date('2020-03-12T09:30').getTime());
  expect(new Date(stored.created_at).getTime()).toBeGreaterThan(
    new Date(stored.occurred_at).getTime(),
  );
  await doctor.getByLabel('Фильтр по типу события', { exact: true }).selectOption('consultation');
  await expect(doctor.locator('.event-timeline .timeline-item')).toHaveCount(1);
  await doctor.getByLabel('Фильтр по типу события', { exact: true }).selectOption('all');
  await doctor.screenshot({ path: 'test-results/retrospective-doctor.png', fullPage: true });

  // Приватная запись и её источник не должны попасть в ответ пациенту.
  const privateId = randomUUID();
  const privateResponse = await doctor.request.post(`/api/v1/patients/${id}/events`, {
    headers: { 'X-Synapse-Request': '1' },
    data: {
      id: privateId,
      title: 'Приватное событие',
      body: 'Скрытое содержание',
      patient_visible: false,
      kind: 'note',
      source: 'Внутренний источник',
    },
  });
  expect(privateResponse.status()).toBe(201);
  await patient.goto('/?demo=1');
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await patient.getByRole('button', { name: 'История наблюдения', exact: true }).click();
  await expect(
    patient.getByRole('heading', { name: 'Консультация в прошлом', exact: true }),
  ).toBeVisible();
  await expect(patient.getByText('Выписка от 12.03.2020', { exact: false })).toBeVisible();
  await expect(patient.getByText('Внутренний источник', { exact: false })).toHaveCount(0);
  const patientRecords = await (await patient.request.get(`/api/v1/patients/${id}/events`)).json();
  expect(patientRecords.some((entry: { id: string }) => entry.id === privateId)).toBe(false);
  await expect
    .poll(() => patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  await patient.screenshot({ path: 'test-results/retrospective-patient.png', fullPage: true });
  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await patient.getByRole('button', { name: 'История наблюдения', exact: true }).click();
  await expect(
    patient.getByRole('heading', { name: 'Консультация в прошлом', exact: true }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  await patientContext.close();
  await doctorContext.close();
});
