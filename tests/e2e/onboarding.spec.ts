import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';

test('врач создаёт карточку, пациент принимает приглашение, изменения защищены от перезаписи', async ({
  browser,
}) => {
  test.setTimeout(60000);
  const doctorContext = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const doctor = await doctorContext.newPage();
  const errors: string[] = [];
  doctor.on('pageerror', (e) => errors.push(e.message));
  await doctor.goto('/?demo=1');
  await doctor.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    doctor.getByRole('heading', {
      name: /^(Доброе утро|Добрый день|Добрый вечер), Иван Сергеевич$/,
      exact: true,
    }),
  ).toBeVisible();
  const navigation = doctor.getByRole('navigation', { name: 'Основные разделы' });
  await navigation.getByRole('button', { name: 'Пациенты' }).click();
  await doctor.getByRole('button', { name: 'Новый пациент', exact: true }).click();
  const name = `Пациент браузерной проверки ${Date.now()}`;
  await doctor.getByLabel('ФИО пациента').fill(name);
  await doctor.getByLabel('Телефон или почта').fill(`e2e-${Date.now()}@synapse.demo`);
  await doctor.getByLabel('Дата рождения').fill('1993-06-15');
  await doctor.getByRole('button', { name: 'Создать приглашение' }).click();
  await expect(doctor.getByRole('heading', { name: 'Подключение пациента' })).toBeVisible();
  const codeField = doctor.getByLabel('Ссылка-приглашение', { exact: true });
  await expect(codeField).toHaveValue(/#invite=[0-9a-f]{64}$/);
  const code = await codeField.inputValue();
  const patientId = await doctor.getByLabel('Выбрать пациента').inputValue();
  await writeFile('test-results/onboarding-fixtures.json', JSON.stringify([patientId]));
  await mkdir('tmp', { recursive: true });
  const previousFixtures = JSON.parse(
    await readFile('tmp/onboarding-e2e-fixtures.json', 'utf8').catch(() => '[]'),
  );
  await writeFile(
    'tmp/onboarding-e2e-fixtures.json',
    JSON.stringify([...previousFixtures, patientId]),
  );
  await doctor.screenshot({ path: 'test-results/invitation-desktop.png', fullPage: true });
  await doctor.getByRole('button', { name: 'Готово', exact: true }).click();
  await expect(doctor.getByRole('heading', { name, exact: true })).toBeVisible();
  await doctor.getByRole('button', { name: 'Редактировать карточку', exact: true }).click();
  await doctor
    .getByLabel('Медицинские сведения', { exact: true })
    .fill('Сведения для проверки истории версий');
  await doctor.getByLabel('Аллергии', { exact: true }).fill('Не выявлено');
  await doctor.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);

  // Второй запрос имитирует другое окно, сохранившее новую версию во время редактирования.
  await doctor.getByRole('button', { name: 'Редактировать карточку', exact: true }).click();
  await doctor.getByLabel('Медицинские сведения', { exact: true }).fill('Несохранённое изменение');
  const profile = await (await doctor.request.get(`/api/v1/patients/${patientId}`)).json();
  const patch = {
    version: profile.version,
    name: profile.name,
    birth_date: profile.birth_date,
    observation_started: profile.observation_started,
    status: profile.status,
    medical_summary: 'Изменено в другом окне',
    allergies: profile.allergies,
  };
  const response = await doctor.request.patch(`/api/v1/patients/${patientId}`, {
    headers: { 'X-Synapse-Request': '1' },
    data: patch,
  });
  expect(response.status()).toBe(200);
  await doctor.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(doctor.getByRole('alert')).toContainText('другом окне');
  await expect(
    doctor.getByRole('button', { name: 'Сохранить карточку', exact: true }),
  ).toBeDisabled();
  await doctor.getByRole('button', { name: 'Загрузить актуальную карточку' }).click();
  await expect(doctor.getByLabel('Медицинские сведения', { exact: true })).toHaveValue(
    'Изменено в другом окне',
  );
  await doctor
    .getByLabel('Медицинские сведения', { exact: true })
    .fill('Согласованная версия карточки');
  await doctor.getByRole('button', { name: 'Сохранить карточку', exact: true }).click();
  await expect(doctor.getByRole('dialog')).toHaveCount(0);
  await navigation.getByRole('button', { name: 'История', exact: true }).click();
  await doctor.getByText('Версия 4', { exact: true }).click();
  await expect(doctor.getByText('Согласованная версия карточки', { exact: false })).toBeVisible();

  const patientContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const patient = await patientContext.newPage();
  patient.on('pageerror', (e) => errors.push(e.message));
  await patient.goto('/' + new URL(code).hash);
  await patient.getByLabel('Пароль', { exact: true }).fill('ПарольПациента2026!');
  await patient.getByLabel('Повторите пароль', { exact: true }).fill('ПарольПациента2026!');
  await patient.getByLabel('Согласен на обработку персональных данных').check();
  await patient.getByRole('button', { name: 'Принять приглашение' }).click();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await expect(
    patient.getByRole('heading', { name: 'Основная информация', exact: true }),
  ).toBeVisible();
  await patient.getByText('Дополнительные сведения', { exact: true }).click();
  await expect(patient.getByText(name, { exact: true })).toBeVisible();
  await expect(patient.getByText('Согласованная версия карточки', { exact: true })).toBeVisible();
  await expect(
    patient.getByRole('button', { name: 'Редактировать карточку', exact: true }),
  ).toHaveCount(0);
  await patient
    .getByRole('navigation', { name: 'Мобильная навигация' })
    .getByRole('button', { name: 'Главная', exact: true })
    .click();
  await patient.getByRole('button', { name: 'Заполнить дневник', exact: true }).click();
  await patient
    .getByLabel('Что ещё важно отметить?', { exact: false })
    .fill('Первая запись после подключения');
  await patient.getByRole('button', { name: 'Сохранить запись', exact: true }).click();
  await expect(patient.getByRole('status').filter({ hasText: 'Запись сохранена' })).toBeVisible();
  await expect
    .poll(() => patient.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    .toBe(true);
  await patient.screenshot({ path: 'test-results/connected-patient-mobile.png', fullPage: true });
  await doctor.getByRole('button', { name: /^Уведомления/ }).click();
  await doctor
    .getByRole('dialog')
    .getByRole('button')
    .filter({ hasText: 'Новая запись дневника' })
    .filter({ hasText: name })
    .click();
  await expect(doctor.getByText('Первая запись после подключения', { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
  await patientContext.close();
  await doctorContext.close();
});
