import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('врач отмечает проявления до выбора диагноза, пациент видит только сохранённую выборку', async ({
  browser,
}) => {
  test.setTimeout(90000);
  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const patientContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const doctor = await doctorContext.newPage(),
    patient = await patientContext.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (error) => errors.push(error.message));
  patient.on('pageerror', (error) => errors.push(error.message));
  const headers = { 'X-Synapse-Request': '1' };
  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    doctor.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  const id = randomUUID();
  const response = await doctor.request.post('/api/v1/patients', {
    headers,
    data: {
      id,
      name: `Пациент клиники ${id.slice(0, 8)}`,
      email: `clinical-e2e-${id}@synapse.demo`,
      birth_date: '1990-01-01',
    },
  });
  expect(response.status()).toBe(201);
  await mkdir('tmp', { recursive: true });
  const previous = JSON.parse(
    await readFile('tmp/clinical-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  await writeFile('tmp/clinical-e2e-fixtures.json', JSON.stringify([...previous, id]));
  const invitation = await (
    await doctor.request.post(`/api/v1/patients/${id}/invitation`, { headers })
  ).json();
  expect(
    (
      await patientContext.request.post('/api/v1/auth/activate', {
        headers,
        data: { code: invitation.code, password: 'Клиника2026!' },
      })
    ).ok(),
  ).toBeTruthy();
  await doctor.reload();
  const nav = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await nav.getByRole('button', { name: 'Медкарта', exact: true }).click();
  await doctor.getByLabel('Выбрать пациента', { exact: true }).selectOption(id);
  await doctor.getByRole('button', { name: 'Добавить диагноз', exact: true }).click();
  const dialog = doctor.getByRole('dialog', { name: 'Новый диагноз', exact: true });
  await expect(dialog.getByLabel('Поиск диагноза', { exact: true })).toBeDisabled();
  const search = dialog.getByLabel('Поиск симптомов и критериев', { exact: true });
  await search.fill('тремор или дрожь');
  const finding = dialog
    .locator('.clinical-finding')
    .filter({ hasText: 'F41.1. PDF, страница 54' })
    .filter({ hasText: 'тремор или дрожь' });
  await expect(finding).toHaveCount(1);
  await finding.getByRole('checkbox').check();
  await search.fill('постоянная раздражительность');
  const second = dialog
    .locator('.clinical-finding')
    .filter({ hasText: 'F41.1. PDF, страница 54' })
    .filter({ hasText: 'постоянная раздражительность' });
  await expect(second).toHaveCount(1);
  await second.getByRole('checkbox').check();
  await dialog.getByLabel('Поиск диагноза', { exact: true }).fill('F41.1');
  await dialog.locator('.clinical-codes button').filter({ hasText: 'F41.1' }).click();
  await dialog
    .getByLabel('Комментарий, видимый пациенту')
    .fill('Синтетическая запись врача для проверки доступа');
  await dialog.getByRole('button', { name: 'Критерии выбранного кода из PDF' }).click();
  await expect(dialog.locator('.clinical-source')).toContainText('шесть месяцев');
  await doctor.screenshot({ path: 'test-results/clinical-doctor.png' });
  await dialog.getByRole('button', { name: 'Сохранить диагноз', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  const record = doctor.locator('.clinical-diagnosis');
  await expect(record).toHaveCount(1);
  await expect(record.locator('.clinical-selected li')).toHaveCount(2);
  const data = await (await doctor.request.get(`/api/v1/patients/${id}/diagnoses`)).json();
  const saved = data.items[0];
  await patient.goto('/?demo=1');
  const patientNav = patient.getByRole('navigation', { name: 'Мобильная навигация' });
  const urls: string[] = [];
  patient.on('request', (request) => urls.push(request.url()));
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(patient.locator('.clinical-diagnosis')).toHaveCount(1);
  await expect(patient.locator('.clinical-selected li')).toHaveCount(2);
  await expect(patient.getByRole('button', { name: 'Добавить диагноз', exact: true })).toHaveCount(
    0,
  );
  await expect(patient.getByLabel('Поиск симптомов и критериев')).toHaveCount(0);
  expect(urls.some((url) => url.includes('/api/v1/clinical/'))).toBe(false);
  for (const suffix of ['findings', 'diagnosis-catalog', 'source/F41.1', 'drugs'])
    expect((await patient.request.get('/api/v1/clinical/' + suffix)).status()).toBe(403);
  const visible = await (
    await patient.request.get(`/api/v1/patients/${id}/diagnoses/${saved.id}`)
  ).json();
  expect(visible.findings).toHaveLength(2);
  expect(JSON.stringify(visible)).not.toContain('ощущение комка в горле');
  await patient.screenshot({ path: 'test-results/clinical-patient-mobile.png', fullPage: true });
  // Изменение в другом окне не перезаписывается устаревшей формой.
  await record.getByRole('button', { name: 'Изменить диагноз' }).click();
  const edit = doctor.getByRole('dialog', { name: 'Изменить диагноз', exact: true });
  const body = {
    id: saved.id,
    version: saved.version,
    catalog_version: saved.catalog_version,
    code: saved.code,
    finding_ids: saved.findings.map((item: { id: string }) => item.id),
    diagnosed_on: saved.diagnosed_on,
    status: saved.status,
    note: 'Изменено другим окном',
  };
  expect(
    (
      await doctor.request.patch(`/api/v1/patients/${id}/diagnoses/${saved.id}`, {
        headers,
        data: body,
      })
    ).ok(),
  ).toBeTruthy();
  await edit.getByLabel('Комментарий, видимый пациенту').fill('Устаревшие сведения');
  await edit.getByRole('button', { name: 'Сохранить диагноз', exact: true }).click();
  await expect(edit.getByRole('alert')).toContainText('другом окне');
  await expect(edit.getByRole('button', { name: 'Сохранить диагноз', exact: true })).toBeDisabled();
  await edit.getByRole('button', { name: 'Загрузить актуальный диагноз' }).click();
  await expect(edit.getByLabel('Комментарий, видимый пациенту')).toHaveValue(
    'Изменено другим окном',
  );
  await edit.locator('.clinical-selection summary').click();
  await edit
    .getByRole('button', { name: 'Убрать: постоянная раздражительность', exact: true })
    .click();
  await edit.getByLabel('Состояние диагноза').selectOption('resolved');
  await edit.getByRole('button', { name: 'Сохранить диагноз', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  await record.getByRole('button', { name: 'История диагноза', exact: true }).click();
  await expect(record.locator('.clinical-audit details')).toHaveCount(3);
  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(patient.locator('.clinical-selected li')).toHaveCount(1);
  await expect(patient.locator('.clinical-diagnosis')).toContainText('Завершён');
  await expect(patient.locator('.clinical-diagnosis')).not.toContainText(
    'постоянная раздражительность',
  );
  expect(
    (await patient.request.get(`/api/v1/patients/${id}/diagnoses/${saved.id}/changes`)).status(),
  ).toBe(403);
  expect(errors).toEqual([]);
  await doctorContext.close();
  await patientContext.close();
});
