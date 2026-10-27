# СИНАПС

Платформа наблюдения за пациентами. Веб-приложение и мобильная оболочка.

## Технологии

- Сервер: Rust, Axum, Tokio, SQLx
- Интерфейс: Svelte 5, TypeScript, Vite
- Мобильное приложение: Tauri 2
- База данных: PostgreSQL 17
- API: REST, спецификация OpenAPI

## Структура

- `apps/frontend` — веб-интерфейс
- `apps/mobile` — Tauri-приложение
- `crates/api` — HTTP API и доступ к данным
- `crates/domain` — общие типы и бизнес-правила
- `migrations` — миграции PostgreSQL
- `clinical` — клинические справочники и правила

## Реализовано для хакатона

- Кабинеты врача и пациента, карточка пациента, анамнез, диагнозы и история изменений.
- Дневник самочувствия, графики, GAD‑7 и PHQ‑9.
- Правила алертов и рекомендации для врачебной оценки; решения остаются за врачом.
- Назначения препаратов, отметки приёма и расчёт приверженности.
- Чат, уведомления и базовое расписание приёмов.
- Управляемые справочники клинических данных с версиями и источниками.

## Ближайшие планы

- Доработать адаптивность и сверку интерфейсов с макетами.
- Расширять проверенные правила рекомендаций и набор клинических справочников.
- Улучшить напоминания пациенту об опросниках и приёме препаратов.

## Локальный запуск

Требуются Rust, Node.js с npm и Docker.

```powershell
Copy-Item .env.example .env
npm ci
docker compose up -d --wait
cargo run -p synapse-api -- seed
cargo run -p synapse-api
```

Во втором терминале:

```powershell
npm run dev
```

- Веб-приложение: <http://localhost:5173>
- API: <http://127.0.0.1:3000>
- Проверка API: <http://127.0.0.1:3000/health>
- OpenAPI: <http://127.0.0.1:3000/api/v1/openapi.json>
- PostgreSQL: `localhost:55432`

## Демо-доступ

Seed создаёт демо-аккаунты при явном запуске команды `cargo run -p synapse-api -- seed`.

- Врач: `doctor@synapse.demo`
- Пациент: `patient@synapse.demo`
- Пароль: `Synapse2026!`

## Мобильное приложение

```powershell
npm run mobile -- dev
```

Сборка и установка Android-приложения на подключённый эмулятор:

```powershell
npm run android
```

## Конфигурация

Локальные параметры находятся в `.env`; шаблон — `.env.example`. Основные переменные: `DATABASE_URL`, `BIND_ADDR`, `ALLOWED_ORIGINS`, `COOKIE_SECURE`, `SYNAPSE_RUNTIME_MODE`.

Локальный PostgreSQL запускается через Docker Compose. Redis и отдельные workers в текущей конфигурации не используются.
