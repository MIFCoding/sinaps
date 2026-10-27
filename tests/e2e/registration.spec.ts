import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

test('вход и регистрация по макету, врач приглашает существующий аккаунт ссылкой', async ({
  browser,
}) => {
  const patientContext = await browser.newContext({ viewport: { width: 390, height: 960 } });
  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const patient = await patientContext.newPage(),
    doctor = await doctorContext.newPage();
  const errors: string[] = [];
  patient.on('pageerror', (error) => errors.push(error.message));
  doctor.on('pageerror', (error) => errors.push(error.message));
  await patient.goto('/');
  await expect(
    patient.getByRole('heading', { name: 'Добро пожаловать!', exact: true }),
  ).toBeVisible();
  await expect(patient.getByLabel('Номер телефона или почта', { exact: true })).toHaveValue('');
  await expect(patient.getByLabel('Пароль', { exact: true })).toHaveValue('');
  await expect(patient.getByText('ДЕМОНСТРАЦИОННЫЙ СТЕНД')).toHaveCount(0);
  await expect(patient.getByText('Подключиться по коду')).toHaveCount(0);
  await expect(patient.getByLabel('Код от врача')).toHaveCount(0);
  const landscape = patient.locator('.auth-landscape');
  await expect(landscape).toBeVisible();
  expect((await landscape.boundingBox())?.height).toBe(100);
  expect((await landscape.boundingBox())?.width).toBe(342);
  await patient.screenshot({ path: 'test-results/design-login-390.png' });
  await patient.getByRole('button', { name: 'Зарегистрироваться', exact: true }).click();
  await expect(patient.getByRole('heading', { name: 'Регистрация', exact: true })).toBeVisible();
  expect(Math.round((await patient.locator('.auth-form').boundingBox())!.width)).toBe(350);
  await patient.screenshot({ path: 'test-results/design-registration-390.png' });
  const email = `design-e2e-${randomUUID()}@synapse.demo`,
    password = 'Дизайн2026!';
  await patient.getByLabel('Номер телефона или почта', { exact: true }).fill(email);
  await patient.getByLabel('Имя', { exact: true }).fill('Дизайн');
  await patient.getByLabel('Фамилия', { exact: true }).fill('Проверка');
  await patient.getByLabel('Пароль', { exact: true }).fill(password);
  await patient.locator('button[aria-controls="auth-password"]').click();
  await expect(patient.getByLabel('Пароль', { exact: true })).toHaveAttribute('type', 'text');
  await patient.getByLabel('Повторите пароль', { exact: true }).fill('НеСовпадает2026');
  await patient.getByLabel('Согласен на обработку персональных данных').check();
  await patient.getByRole('button', { name: 'Создать аккаунт', exact: true }).click();
  await expect(patient.getByRole('alert')).toHaveText('Пароли не совпадают');
  await patient.getByLabel('Повторите пароль', { exact: true }).fill(password);
  const registration = patient.waitForResponse(
    (response) =>
      response.url().endsWith('/auth/register') && response.request().method() === 'POST',
  );
  await patient.getByRole('button', { name: 'Создать аккаунт', exact: true }).click();
  const response = await registration;
  expect(response.status()).toBe(201);
  const user = await response.json();
  await mkdir('tmp', { recursive: true });
  const fixture = 'tmp/registration-e2e-fixtures.json';
  await writeFile(
    fixture,
    JSON.stringify([...JSON.parse(await readFile(fixture, 'utf8').catch(() => '[]')), user.id]),
  );
  await expect(patient.getByRole('heading', { name: 'Наблюдение ещё не начато' })).toBeVisible();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(patient.locator('.account-profile')).toContainText(email);
  await expect(
    patient.getByRole('heading', { name: 'Дизайн Проверка', exact: true }),
  ).toBeVisible();
  await patient.locator('.topbar-logout').click();
  await expect(
    patient.getByRole('heading', { name: 'Добро пожаловать!', exact: true }),
  ).toBeVisible();
  await patient.getByLabel('Номер телефона или почта', { exact: true }).fill(email);
  await patient.getByLabel('Пароль', { exact: true }).fill(password);
  await patient.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(patient.getByRole('heading', { name: 'Наблюдение ещё не начато' })).toBeVisible();
  await doctor.goto('/?demo=1');
  await expect(doctor.getByLabel('Номер телефона или почта', { exact: true })).toHaveValue(
    'doctor@synapse.demo',
  );
  const doctorLogin = doctor.waitForResponse(
    (response) => response.url().endsWith('/auth/login') && response.request().method() === 'POST',
  );
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  expect((await doctorLogin).status()).toBe(200);
  const nav = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await nav.getByRole('button', { name: /^Пациенты/ }).click();
  await doctor.getByRole('button', { name: 'Новый пациент', exact: true }).click();
  await doctor.getByLabel('ФИО пациента', { exact: true }).fill('Дизайн Проверка');
  await doctor.getByLabel('Телефон или почта', { exact: true }).fill(email);
  await doctor.getByLabel('Дата рождения', { exact: true }).fill('1990-01-01');
  await doctor.getByRole('button', { name: 'Создать приглашение', exact: true }).click();
  const link = doctor.getByLabel('Ссылка-приглашение', { exact: true });
  await expect(link).toHaveValue(/#invite=[a-f0-9]{64}$/);
  const invitation = await link.inputValue();
  await doctor.getByRole('button', { name: 'Готово', exact: true }).click();
  expect((await patient.request.get(`/api/v1/patients/${user.id}`)).status()).toBe(403);
  await doctor.getByRole('button', { name: 'Редактировать карточку', exact: true }).click();
  await doctor.getByLabel('Рост, см', { exact: true }).fill('168');
  await doctor.getByLabel('Вес, кг', { exact: true }).fill('60.5');
  await doctor.getByLabel('Группа крови', { exact: true }).selectOption('O (I) Rh+');
  await doctor.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  await patient.goto('/' + new URL(invitation).hash);
  await expect(patient.getByText('Приглашение от врача:', { exact: false })).toContainText(
    'Иван Сергеевич',
  );
  await expect(patient.getByLabel('Повторите пароль', { exact: true })).toHaveCount(0);
  await patient.getByLabel('Пароль', { exact: true }).fill(password);
  await patient.getByLabel('Согласен на обработку персональных данных').check();
  await patient.getByRole('button', { name: 'Принять приглашение', exact: true }).click();
  const patientNav = patient.getByRole('navigation', { name: 'Мобильная навигация' });
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  const info = patient.locator('.basic-information');
  await expect(info).toContainText('168 см');
  await expect(info).toContainText('60,5 кг');
  await expect(info).toContainText('O (I) Rh+');
  await patient.screenshot({ path: 'test-results/design-medical-390.png' });
  await expect(patientNav.getByRole('button')).toHaveCount(5);
  await expect(patientNav.getByRole('button', { name: 'Сообщения', exact: true })).toBeEnabled();
  await patientNav.getByRole('button', { name: 'Врачи', exact: true }).click();
  await expect(patient.getByRole('heading', { name: 'Иван Сергеевич', exact: true })).toBeVisible();
  expect(new URL(patient.url()).hash).toBe('');
  expect(
    await patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  expect(errors).toEqual([]);
  await doctorContext.close();
  await patientContext.close();
});
