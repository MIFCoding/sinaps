import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('анамнез сохраняется с аудитом, пациент читает сведения, конфликт не теряет изменения', async ({
  browser,
}) => {
  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const patientContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const doctor = await doctorContext.newPage(),
    patient = await patientContext.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (error) => errors.push(error.message));
  patient.on('pageerror', (error) => errors.push(error.message));
  const headers = { 'X-Synapse-Request': '1' };
  const id = randomUUID(),
    path = `/api/v1/patients/${id}`;
  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    doctor.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  const created = await doctor.request.post('/api/v1/patients', {
    headers,
    data: {
      id,
      name: `Анамнез ${id.slice(0, 8)}`,
      email: `anamnesis-e2e-${id}@synapse.demo`,
      birth_date: '1990-01-01',
    },
  });
  expect(created.status()).toBe(201);
  await mkdir('tmp', { recursive: true });
  const fixture = 'tmp/medical-history-e2e-fixtures.json';
  const previous = JSON.parse(await readFile(fixture, 'utf8').catch(() => '[]'));
  await writeFile(fixture, JSON.stringify([...previous, id]));
  const invitation = await (await doctor.request.post(path + '/invitation', { headers })).json();
  expect(
    (
      await patientContext.request.post('/api/v1/auth/activate', {
        headers,
        data: {
          code: invitation.code,
          password: 'Анамнез2026!',
        },
      })
    ).ok(),
  ).toBeTruthy();
  await doctor.reload();
  const nav = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await nav.getByRole('button', { name: 'Медкарта', exact: true }).click();
  await doctor.getByLabel('Выбрать пациента', { exact: true }).selectOption(id);
  const card = doctor.locator('.medical-history');
  await expect(card).toContainText('Нет сведений');
  await doctor.getByRole('button', { name: 'Редактировать карточку', exact: true }).click();
  const dialog = doctor.getByRole('dialog', { name: 'Редактировать карточку', exact: true });
  await dialog
    .getByLabel('Жалобы', { exact: true })
    .fill('Жалобы со слов пациента\nСинтетические сведения');
  await dialog
    .getByLabel('История текущего состояния', { exact: true })
    .fill('Начало и течение со слов пациента');
  await dialog.getByLabel('Анамнез жизни', { exact: true }).fill('Сведения об анамнезе жизни');
  await dialog.getByLabel('Сопутствующие состояния', { exact: true }).selectOption('present');
  await dialog
    .getByLabel('Описание сопутствующих состояний', { exact: true })
    .fill('Синтетические сведения из выписки');
  await dialog.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  await expect(card).toContainText('Синтетические сведения из выписки');
  await doctor.reload();
  await nav.getByRole('button', { name: 'Медкарта', exact: true }).click();
  await doctor.getByLabel('Выбрать пациента', { exact: true }).selectOption(id);
  await expect(card).toContainText('Начало и течение со слов пациента');
  await patient.goto('/?demo=1');
  const patientNav = patient.getByRole('navigation', { name: 'Мобильная навигация' });
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  const patientCard = patient.locator('.medical-history');
  await expect(patientCard).toContainText('Синтетические сведения из выписки');
  await expect(
    patient.getByRole('button', { name: 'Редактировать карточку', exact: true }),
  ).toHaveCount(0);
  const saved = await (await doctor.request.get(path)).json();
  expect((await patient.request.patch(path, { headers, data: saved })).status()).toBe(403);
  expect((await patient.request.get(path + '/changes/page')).status()).toBe(403);
  await doctor.getByRole('button', { name: 'Редактировать карточку', exact: true }).click();
  await dialog.getByLabel('Жалобы', { exact: true }).fill('Устаревшие несохранённые жалобы');
  expect(
    (
      await doctor.request.patch(path, {
        headers,
        data: {
          ...saved,
          medical_history: {
            ...saved.medical_history,
            complaints: 'Актуальные жалобы из другого окна',
          },
        },
      })
    ).ok(),
  ).toBeTruthy();
  await dialog.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('другом окне');
  await expect(dialog.getByLabel('Жалобы', { exact: true })).toHaveValue(
    'Устаревшие несохранённые жалобы',
  );
  await expect(
    dialog.getByRole('button', { name: 'Сохранить карточку', exact: true }),
  ).toBeDisabled();
  await dialog.getByRole('button', { name: 'Загрузить актуальную карточку' }).click();
  await expect(dialog.getByLabel('Жалобы', { exact: true })).toHaveValue(
    'Актуальные жалобы из другого окна',
  );
  await dialog.getByLabel('Сопутствующие состояния', { exact: true }).selectOption('none');
  await expect(dialog.getByLabel('Описание сопутствующих состояний')).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  await expect(card).toContainText('Сопутствующие состояния не выявлены');
  await expect(card).not.toContainText('Синтетические сведения из выписки');
  await nav.getByRole('button', { name: 'История', exact: true }).click();
  const history = doctor.locator('.profile-history');
  const revision = history.locator('.profile-change').first();
  await revision.locator('summary').click();
  await expect(revision).toContainText('Синтетические сведения из выписки');
  await expect(revision).toContainText('Сопутствующие состояния не выявлены');
  await expect(revision).toContainText('Иван Сергеевич');
  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(patientCard).toContainText('Сопутствующие состояния не выявлены');
  await expect(patientCard).not.toContainText('Синтетические сведения из выписки');
  await patientCard.scrollIntoViewIfNeeded();
  await patient.screenshot({ path: 'test-results/medical-history-mobile.png' });
  expect(
    await patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  expect(errors).toEqual([]);
  await doctorContext.close();
  await patientContext.close();
});
