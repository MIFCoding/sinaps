import { test, expect, type Page } from '@playwright/test';

async function login(page: Page, role: 'doctor' | 'patient') {
  await page.goto('/?demo=1');
  await page
    .getByRole('button', { name: role === 'doctor' ? 'Я врач' : 'Я пациент', exact: true })
    .click();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    page.getByRole('heading', {
      name:
        role === 'doctor'
          ? /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/
          : /^(Доброе утро|Добрый день|Добрый вечер), Анна$/,
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole('heading', {
      name: role === 'doctor' ? 'Мои пациенты' : 'Быстрый доступ',
      exact: true,
    }),
  ).toBeVisible();
}

test('mobile patient diary reaches doctor and survives reload', async ({ browser }) => {
  const patientContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const patient = await patientContext.newPage();
  const errors: string[] = [];
  patient.on('pageerror', (e) => errors.push(e.message));
  await login(patient, 'patient');
  await expect
    .poll(() => patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  await patient.screenshot({ path: 'test-results/patient-mobile.png', fullPage: true });
  await patient.getByRole('button', { name: 'Заполнить дневник', exact: true }).click();
  const comment = `Сквозная проверка ${Date.now()}`;
  await patient.getByLabel('Что ещё важно отметить?', { exact: false }).fill(comment);
  await patient.getByRole('button', { name: 'Сохранить запись' }).click();
  await expect(patient.getByRole('status').filter({ hasText: 'Запись сохранена' })).toBeVisible();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await patient.getByRole('button', { name: 'История наблюдения', exact: true }).click();
  await expect(patient.getByText(comment, { exact: true })).toBeVisible();
  await patient.reload();
  await expect(
    patient.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Анна$/,
      exact: true,
    }),
  ).toBeVisible();

  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const doctor = await doctorContext.newPage();
  doctor.on('pageerror', (e) => errors.push(e.message));
  await login(doctor, 'doctor');
  await doctor
    .getByRole('navigation', { name: 'Основные разделы' })
    .getByRole('button', { name: 'История' })
    .click();
  await expect(doctor.getByText(comment, { exact: true })).toBeVisible();
  await doctor.getByRole('button', { name: 'Добавить заметку' }).click();
  await doctor.getByLabel('Заголовок', { exact: true }).fill('Внутренняя заметка E2E');
  const secret = `Только врачу ${Date.now()}`;
  await doctor.getByLabel('Запись врача', { exact: true }).fill(secret);
  await doctor.getByRole('button', { name: 'Сохранить запись' }).click();
  await expect(doctor.getByText(secret, { exact: true })).toBeVisible();
  await doctor.screenshot({ path: 'test-results/doctor-desktop.png', fullPage: true });
  await patient.reload();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await patient.getByRole('button', { name: 'История наблюдения', exact: true }).click();
  await expect(patient.getByText(secret, { exact: true })).toHaveCount(0);
  await expect
    .poll(() => doctor.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  expect(errors).toEqual([]);
  await patientContext.close();
  await doctorContext.close();
});

test('doctor filters patients and opens an empty history without invented measurements', async ({
  page,
}) => {
  await page.setViewportSize({ width: 834, height: 1000 });
  await login(page, 'doctor');
  await page
    .getByRole('navigation', { name: 'Основные разделы' })
    .getByRole('button', { name: 'Пациенты' })
    .click();
  await page.getByLabel('Поиск пациента').fill('Мария');
  await page.getByRole('button', { name: /Мария Соколова/ }).click();
  await expect(page.getByRole('heading', { name: 'Мария Соколова', exact: true })).toBeVisible();
  await page
    .getByRole('navigation', { name: 'Основные разделы' })
    .getByRole('button', { name: 'Динамика', exact: true })
    .click();
  await expect(page.getByText('За этот период измерений пока нет')).toHaveCount(3);
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  await page.screenshot({ path: 'test-results/doctor-tablet.png', fullPage: true });
});
