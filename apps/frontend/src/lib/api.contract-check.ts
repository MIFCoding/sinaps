import { api, type ApiOptions, type ApiResult, type Patient } from './api';

// Функция не вызывается приложением; svelte-check проверяет границы контракта.
export function checkApiContract() {
  const patient: Promise<Patient> = api('get_patient', { params: { id: 'пример-uuid' } });
  const logout: Promise<void> = api('logout');
  const query: ApiOptions<'list_patients_page'> = { query: { limit: 20 } };
  const version: ApiResult<'list_profile_changes_page'>['next_cursor'] = { before_version: 2 };
  void [patient, logout, query, version];

  // @ts-expect-error Идентификатор пациента обязателен в пути.
  api('get_patient');
  // @ts-expect-error Числовой лимит нельзя передать строкой.
  api('list_patients_page', { query: { limit: '20' } });
  // @ts-expect-error Операция чтения не принимает тело PATCH.
  api('get_patient', { params: { id: 'пример-uuid' }, body: { name: 'Анна' } });
  // @ts-expect-error Логин требует пароль, описанный серверным типом.
  api('login', { body: { email: 'doctor@example.test' } });
  // @ts-expect-error Неизвестная операция отсутствует в реестре сервера.
  api('delete_patient');
  // @ts-expect-error Ответ карточки нельзя выдать за строку.
  const wrongResponse: Promise<string> = api('get_patient', { params: { id: 'пример-uuid' } });
  // @ts-expect-error Тип события берётся из серверного перечисления.
  const wrongKind: ApiResult<'get_event'>['kind'] = 'unknown';
  void [wrongResponse, wrongKind];
}
