import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('пациент не отправляет сообщения о реакциях, врач принимает алерт GAD-7 в работу', async ({
  browser,
}) => {
  test.setTimeout(90000);
  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const patientContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const doctor = await doctorContext.newPage();
  const patient = await patientContext.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (error) => errors.push(error.message));
  patient.on('pageerror', (error) => errors.push(error.message));
  const headers = { 'X-Synapse-Request': '1' };
  const id = randomUUID();
  const name = `Алерт E2E ${id.slice(0, 8)}`;
  const base = `/api/v1/patients/${id}`;

  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(doctor.locator('.home-greeting')).toContainText('Иван Сергеевич');
  expect(
    (
      await doctor.request.post('/api/v1/patients', {
        headers,
        data: { id, name, email: `alert-e2e-${id}@synapse.demo`, birth_date: '1990-01-01' },
      })
    ).status(),
  ).toBe(201);
  await mkdir('tmp', { recursive: true });
  const fixture = 'tmp/alert-e2e-fixtures.json';
  await writeFile(
    fixture,
    JSON.stringify([...JSON.parse(await readFile(fixture, 'utf8').catch(() => '[]')), id]),
  );
  const invitation = await (await doctor.request.post(base + '/invitation', { headers })).json();
  expect(
    (
      await patientContext.request.post('/api/v1/auth/activate', {
        headers,
        data: { code: invitation.code, password: 'Алерт2026!' },
      })
    ).status(),
  ).toBe(200);

  await patient.goto('/');
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  const therapy = patient.locator('.therapy-panel');
  await expect(therapy.getByRole('button', { name: 'Сообщить о нежелательном явлении' })).toHaveCount(0);
  const patientReport = await patient.request.post(base + '/adverse-events', {
    headers,
    data: {
      id: randomUUID(),
      prescription_id: randomUUID(),
      occurred_at: new Date().toISOString(),
      severity: 'severe',
      effect_items: ['Синтетическая реакция'],
      note: '',
    },
  });
  expect(patientReport.status()).toBe(403);

  const definition = await (await doctor.request.get('/api/v1/questionnaires/gad7')).json();
  const assignmentId = randomUUID();
  expect(
    (
      await doctor.request.post(base + '/questionnaires', {
        headers,
        data: { id: assignmentId, scale_version: definition.version, note: 'Синтетический E2E тест' },
      })
    ).status(),
  ).toBe(201);
  expect(
    (
      await patient.request.post(`${base}/questionnaires/${assignmentId}/submit`, {
        headers,
        data: { version: 1, scale_version: definition.version, answers: [3, 3, 3, 3, 3, 0, 0] },
      })
    ).status(),
  ).toBe(200);
  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  const patientAlert = patient.locator('.alert-panel .critical-alert').first();
  await expect(patientAlert).toContainText('КРИТИЧЕСКИЙ АЛЕРТ');
  await expect(patientAlert).toContainText('Опросник не ставит диагноз');
  const page = await (await patient.request.get(base + '/alerts')).json();
  expect(page.items).toHaveLength(1);
  const alert = page.items[0];
  expect(alert.status).toBe('new');
  expect((await patient.request.get(base + '/prescriptions')).status()).toBe(200);
  expect(await (await patient.request.get(base + '/prescriptions')).json()).toEqual([]);

  await doctor.reload();
  await doctor.getByRole('button', { name: /Уведомления/ }).click();
  const notifications = doctor.getByRole('dialog', { name: 'Уведомления' });
  const criticalNotification = notifications
    .locator('.notification-item')
    .filter({ hasText: 'Критический алерт' })
    .filter({ hasText: name });
  await expect(criticalNotification).toBeVisible();
  await criticalNotification.click();
  const doctorAlert = doctor.locator(`#alert-${alert.id}`);
  await expect(doctorAlert).toBeVisible();
  expect((await (await doctor.request.get(base + '/alerts')).json()).items[0].status).toBe('new');
  await doctorAlert.getByRole('button', { name: 'Принять в работу', exact: true }).click();
  await expect(doctorAlert).toContainText('Принят в работу');

  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(patient.locator(`#alert-${alert.id}`)).toContainText('Принят в работу');
  expect(errors).toEqual([]);
  await doctorContext.close();
  await patientContext.close();
});
