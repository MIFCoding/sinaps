import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('врач назначает GAD-7, пациент отправляет полные ответы, история и отмена сохраняются', async ({
  browser,
}) => {
  test.setTimeout(90000);
  const dc = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const pc = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const doctor = await dc.newPage(),
    patient = await pc.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (e) => errors.push(e.message));
  patient.on('pageerror', (e) => errors.push(e.message));
  const headers = { 'X-Synapse-Request': '1' },
    id = randomUUID(),
    name = `Опросник E2E ${id.slice(0, 8)}`,
    base = `/api/v1/patients/${id}`;
  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(doctor.locator('.home-greeting')).toContainText('Иван Сергеевич');
  expect(
    (
      await doctor.request.post('/api/v1/patients', {
        headers,
        data: { id, name, email: `questionnaire-e2e-${id}@synapse.demo`, birth_date: '1990-01-01' },
      })
    ).status(),
  ).toBe(201);
  await mkdir('tmp', { recursive: true });
  const fixture = 'tmp/questionnaire-e2e-fixtures.json';
  await writeFile(
    fixture,
    JSON.stringify([...JSON.parse(await readFile(fixture, 'utf8').catch(() => '[]')), id]),
  );
  const invitation = await (await doctor.request.post(base + '/invitation', { headers })).json();
  expect(
    (
      await pc.request.post('/api/v1/auth/activate', {
        headers,
        data: { code: invitation.code, password: 'Опросник2026!' },
      })
    ).status(),
  ).toBe(200);
  const nav = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await nav.getByRole('button', { name: /^Пациенты/ }).click();
  await doctor.getByLabel('Поиск пациента').fill(name);
  await doctor.locator('.patient-list-card .person-row').filter({ hasText: name }).click();
  const dp = doctor.locator('.questionnaires'),
    pp = patient.locator('.questionnaires');
  async function assign(note: string) {
    await dp.getByLabel('Поиск опросника').fill('GAD');
    await dp.getByLabel('Опросник').selectOption('gad7');
    await dp.getByLabel('Комментарий пациенту').fill(note);
    await dp.getByRole('button', { name: 'Назначить', exact: true }).click();
    await expect(dp.locator('.questionnaire-record').first()).toContainText(note);
  }
  await assign('Первичная оценка по самоотчёту');
  const assigned = (await (await doctor.request.get(base + '/questionnaires')).json()).items[0];
  expect(
    (
      await doctor.request.post(base + `/questionnaires/${assigned.id}/submit`, {
        headers,
        data: { version: 1, scale_version: assigned.scale_version, answers: [0, 0, 0, 0, 0, 0, 0] },
      })
    ).status(),
  ).toBe(403);
  await patient.goto('/');
  await expect(pp.getByRole('button', { name: /^Заполнить GAD/ })).toBeVisible();
  await expect(pp.getByRole('button', { name: 'Назначить', exact: true })).toHaveCount(0);
  expect((await patient.request.get('/api/v1/questionnaires/gad7')).status()).toBe(403);
  expect(
    (await patient.request.get(base + `/questionnaires/${assigned.id}/changes`)).status(),
  ).toBe(403);
  const pn = patient.getByRole('navigation', { name: 'Мобильная навигация' });
  async function fill(values: number[]) {
    await pp.getByRole('button', { name: /^Заполнить GAD/ }).click();
    const form = patient.getByRole('dialog', { name: /Заполнение: GAD/ });
    await expect(
      form.getByRole('button', { name: 'Отправить ответы', exact: true }),
    ).toBeDisabled();
    for (let i = 0; i < values.length; i++)
      await form
        .locator('.questionnaire-question')
        .nth(i)
        .getByRole('radio')
        .nth(values[i])
        .check();
    return form;
  }
  let form = await fill([0, 1, 2, 3, 0, 1]);
  await expect(form.getByRole('button', { name: 'Отправить ответы', exact: true })).toBeDisabled();
  await form.locator('.questionnaire-question').nth(6).getByRole('radio').nth(2).check();
  let failed = false;
  await patient.route(`**${base}/questionnaires/${assigned.id}/submit`, async (route) => {
    if (!failed) {
      failed = true;
      await route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({
          error: { code: 'internal_error', message: 'Проверка повтора отправки' },
        }),
      });
    } else await route.continue();
  });
  await form.getByRole('button', { name: 'Отправить ответы', exact: true }).click();
  await expect(form.getByRole('alert')).toContainText('Проверка повтора отправки');
  await expect(form.locator('input:checked')).toHaveCount(7);
  await patient.screenshot({ path: 'test-results/questionnaire-form-mobile.png' });
  await form.getByRole('button', { name: 'Отправить ответы', exact: true }).click();
  await expect(patient.getByRole('dialog')).toHaveCount(0);
  await expect(pp.locator('.questionnaire-score')).toContainText('9 / 21');
  await nav.getByRole('button', { name: 'Динамика', exact: true }).click();
  await expect(dp.locator('.questionnaire-score')).toContainText('9 / 21');
  await expect(dp.locator('.questionnaire-trend circle')).toHaveCount(1);
  await dp.getByRole('button', { name: 'Журнал опросника', exact: true }).click();
  await expect(dp.locator('.questionnaire-audit')).toContainText('Версия 2');
  await expect(dp.locator('.questionnaire-audit')).toContainText(name);
  await assign('Повторная оценка');
  await pn.getByRole('button', { name: 'Динамика', exact: true }).click();
  form = await fill([3, 3, 3, 3, 3, 3, 3]);
  await form.getByRole('button', { name: 'Отправить ответы', exact: true }).click();
  await expect(patient.getByRole('dialog')).toHaveCount(0);
  await expect(pp.locator('.questionnaire-trend circle')).toHaveCount(2);
  await expect(pp.locator('.questionnaire-score').first()).toContainText('21 / 21');
  await assign('Назначение для проверки отмены');
  await pn.getByRole('button', { name: 'Главная', exact: true }).click();
  form = await fill([0, 0, 0, 0, 0, 0, 0]);
  await dp.getByRole('button', { name: 'Отменить назначение', exact: true }).click();
  await expect(dp.locator('.questionnaire-record').first()).toContainText('Отменён');
  await form.getByRole('button', { name: 'Отправить ответы', exact: true }).click();
  await expect(form.getByRole('alert')).toContainText('отменено');
  await expect(form.locator('input:checked')).toHaveCount(7);
  await expect(form.getByRole('button', { name: 'Отправить ответы', exact: true })).toBeDisabled();
  await form.getByRole('button', { name: 'Загрузить актуальное назначение', exact: true }).click();
  await expect(patient.getByRole('dialog')).toHaveCount(0);
  await expect(pp.locator('.questionnaire-record').first()).toContainText('Отменён');
  await expect(pp.getByRole('button', { name: /^Заполнить GAD/ })).toHaveCount(0);
  await doctor.screenshot({ path: 'test-results/questionnaire-doctor.png' });
  let release = () => {},
    delayed = false,
    finished = false;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await doctor.route(`**${base}/questionnaires*`, async (route) => {
    if (
      route.request().method() !== 'GET' ||
      new URL(route.request().url()).pathname !== base + '/questionnaires'
    ) {
      await route.continue();
      return;
    }
    delayed = true;
    const response = await route.fetch();
    await gate;
    await route.fulfill({ response });
    finished = true;
  });
  await nav.getByRole('button', { name: 'Медкарта', exact: true }).click();
  await expect.poll(() => delayed).toBe(true);
  await doctor
    .getByLabel('Выбрать пациента', { exact: true })
    .selectOption('00000000-0000-0000-0000-000000000002');
  await expect(dp.locator('.questionnaire-record')).toHaveCount(0);
  release();
  await expect.poll(() => finished).toBe(true);
  await expect(dp.locator('.questionnaire-record')).toHaveCount(0);
  await expect(dp).not.toContainText('Первичная оценка по самоотчёту');
  expect(
    await patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  expect(errors).toEqual([]);
  await dc.close();
  await pc.close();
});
