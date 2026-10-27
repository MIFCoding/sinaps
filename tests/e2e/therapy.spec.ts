import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('врач публикует назначение, опрашивает пациента и фиксирует выявленную реакцию', async ({
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
  const name = `Терапия E2E ${id.slice(0, 8)}`;
  const base = `/api/v1/patients/${id}`;

  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(doctor.locator('.home-greeting')).toContainText('Иван Сергеевич');
  expect(
    (
      await doctor.request.post('/api/v1/patients', {
        headers,
        data: { id, name, email: `therapy-e2e-${id}@synapse.demo`, birth_date: '1990-01-01' },
      })
    ).status(),
  ).toBe(201);
  await mkdir('tmp', { recursive: true });
  const fixture = 'tmp/therapy-e2e-fixtures.json';
  await writeFile(
    fixture,
    JSON.stringify([...JSON.parse(await readFile(fixture, 'utf8').catch(() => '[]')), id]),
  );
  const codeCatalog = await (await doctor.request.get('/api/v1/clinical/diagnosis-catalog?search=F41.1')).json();
  const findingCatalog = await (await doctor.request.get('/api/v1/clinical/findings?search=F41.1')).json();
  const anxietyFinding = findingCatalog.items.find((item: { source_code: string }) => item.source_code === 'F41.1');
  expect(anxietyFinding).toBeTruthy();
  expect(
    (
      await doctor.request.post(base + '/diagnoses', {
        headers,
        data: {
          id: randomUUID(),
          version: 1,
          catalog_version: codeCatalog.version,
          code: 'F41.1',
          finding_ids: [anxietyFinding.id],
          diagnosed_on: new Date().toISOString().slice(0, 10),
          status: 'active',
          note: 'Синтетический активный диагноз для E2E.',
        },
      })
    ).status(),
  ).toBe(201);
  const invitation = await (await doctor.request.post(base + '/invitation', { headers })).json();
  expect(
    (
      await patientContext.request.post('/api/v1/auth/activate', {
        headers,
        data: { code: invitation.code, password: 'Терапия2026!' },
      })
    ).status(),
  ).toBe(200);

  const doctorNavigation = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await doctorNavigation.getByRole('button', { name: /^Пациенты/ }).click();
  await doctor.getByLabel('Поиск пациента').fill(name);
  await doctor.getByRole('button', { name: new RegExp(name) }).click();
  await doctor.getByRole('button', { name: 'Лечение', exact: true }).click();
  const doctorPanel = doctor.locator('.therapy-panel');
  await doctorPanel.getByRole('button', { name: 'Новое назначение', exact: true }).click();
  const editor = doctor.getByRole('dialog', { name: 'Новое назначение' });
  await editor.getByLabel('Сначала диагноз').selectOption({ label: /F41\.1/ });
  await editor.getByLabel('Подходящий препарат').selectOption('aviandr');
  await editor.getByLabel('Дозировка и форма').fill('Синтетическая схема E2E');
  await editor.getByLabel('Время приёма через запятую').fill('09:00, 21:00');
  await editor
    .getByLabel('Инструкции пациенту')
    .fill('Проверка назначения без клинического применения.');
  await editor.getByRole('button', { name: 'Сохранить черновик', exact: true }).click();
  await expect(editor).toHaveCount(0);
  const prescription = doctorPanel.locator('.therapy-item').first();
  await expect(prescription).toContainText('Черновик');
  await expect(prescription).toContainText('F41.1');

  await patient.goto('/');
  const patientNavigation = patient.getByRole('navigation', { name: 'Мобильная навигация' });
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  const patientPanel = patient.locator('.therapy-panel');
  await expect(patientPanel).toContainText('Нет опубликованных назначений');
  await prescription
    .getByRole('button', { name: 'Подтвердить и опубликовать', exact: true })
    .click();
  await expect(prescription).toContainText('Действует');

  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  const active = patientPanel.locator('.therapy-item').first();
  await expect(active).toContainText('Синтетическая схема E2E');
  await expect(active).toContainText('09:00, 21:00');
  await active.getByRole('button', { name: 'Принято', exact: true }).first().click();
  await expect(active.locator('.intake-row').first()).toContainText('Принято');
  await expect(active.locator('.adherence-summary')).toContainText('Принято: 1');
  await expect(patientPanel.getByRole('button', { name: 'Сообщить о нежелательном явлении' })).toHaveCount(0);
  const blockedReport = await patient.request.post(base + '/adverse-events', {
    headers,
    data: {
      id: randomUUID(),
      prescription_id: randomUUID(),
      occurred_at: new Date().toISOString(),
      severity: 'moderate',
      effect_items: ['Синтетическая реакция'],
      note: '',
    },
  });
  expect(blockedReport.status()).toBe(403);

  await doctor.reload();
  await doctorNavigation.getByRole('button', { name: /^Пациенты/ }).click();
  await doctor.getByLabel('Поиск пациента').fill(name);
  await doctor.getByRole('button', { name: new RegExp(name) }).click();
  await doctor.getByRole('button', { name: 'Лечение', exact: true }).click();
  const doctorActivePanel = doctor.locator('.therapy-panel');
  const prescriptions = await (await doctor.request.get(base + '/prescriptions')).json();
  const effect = prescriptions[0].drug_snapshot.effects[0].items[0];
  await doctorActivePanel.getByRole('button', { name: 'Зафиксировать выявленные реакции' }).click();
  const adverse = doctor.getByRole('dialog', { name: 'Фиксация выявленных реакций' });
  await adverse.getByLabel(effect).check();
  await adverse.getByLabel('Выраженность').selectOption('moderate');
  await adverse.getByLabel('Комментарий врача, если нужен').fill('Синтетическая запись E2E после опроса.');
  await adverse.getByRole('button', { name: 'Сохранить запись', exact: true }).click();
  await expect(adverse).toHaveCount(0);
  const event = doctor.locator('.therapy-panel .adverse-item').first();
  await expect(event).toContainText(effect);
  await expect(event).toContainText('Выявлено врачом');
  await expect(event).toContainText('Синтетическая запись E2E после опроса.');
  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(patient.locator('.therapy-panel .adverse-item').first()).toContainText(effect);

  const current = doctor.locator('.therapy-panel .therapy-item').first();
  await current.getByRole('button', { name: 'История назначения', exact: true }).click();
  await expect(current.locator('.clinical-audit')).toContainText('published');
  await current.getByRole('button', { name: 'Отменить назначение', exact: true }).first().click();
  await expect(current).toContainText('Отменено');
  expect(errors).toEqual([]);
  await doctorContext.close();
  await patientContext.close();
});
