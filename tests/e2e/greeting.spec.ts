import { test, expect } from '@playwright/test';

test('приветствие меняется на границах дня и после возврата, заголовки не дублируют меню', async ({
  browser,
}) => {
  const context = await browser.newContext({
    timezoneId: 'Europe/Moscow',
    viewport: { width: 390, height: 844 },
  });
  const page = await context.newPage();
  await page.clock.install({ time: new Date('2026-10-26T04:59:30+03:00') });
  await page.goto('/?demo=1');
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Добрый вечер, Иван Сергеевич', exact: true }),
  ).toBeVisible();
  await page.clock.fastForward(60000);
  await expect(
    page.getByRole('heading', { name: 'Доброе утро, Иван Сергеевич', exact: true }),
  ).toBeVisible();
  for (const [time, greeting] of [
    ['11:59:00', 'Доброе утро'],
    ['12:00:00', 'Добрый день'],
    ['17:59:00', 'Добрый день'],
    ['18:00:00', 'Добрый вечер'],
    ['22:59:00', 'Добрый вечер'],
    ['23:00:00', 'Добрый вечер'],
    ['00:00:00', 'Добрый вечер'],
  ]) {
    await page.clock.setSystemTime(new Date(`2026-10-26T${time}+03:00`));
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(
      page.getByRole('heading', { name: `${greeting}, Иван Сергеевич`, exact: true }),
    ).toBeVisible();
  }
  await expect(page.locator('.topbar-title, .page-heading')).toHaveCount(0);
  const nav = page.getByRole('navigation', { name: 'Мобильная навигация' });
  await nav.getByRole('button', { name: 'Пациенты', exact: true }).click();
  await expect(page.locator('.directory-result-count')).toContainText('Найдено:');
  await expect(nav.getByRole('button', { name: 'Пациенты', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.locator('.page-heading, .topbar-title')).toHaveCount(0);
  await context.close();

  const otherZone = await browser.newContext({
    timezoneId: 'America/Los_Angeles',
    viewport: { width: 390, height: 844 },
  });
  const patient = await otherZone.newPage();
  await patient.clock.install({ time: new Date('2026-10-26T17:00:00Z') });
  await patient.goto('/?demo=1');
  await patient.getByRole('button', { name: 'Пациент', exact: true }).click();
  await patient.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(
    patient.getByRole('heading', { name: 'Доброе утро, Анна', exact: true }),
  ).toBeVisible();
  await patient.getByRole('button', { name: 'Открыть профиль', exact: true }).click();
  await patient.getByRole('button', { name: 'История наблюдения', exact: true }).click();
  await expect(
    patient
      .getByRole('navigation', { name: 'Мобильная навигация' })
      .getByRole('button', { name: 'Динамика', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await otherZone.close();
});
