import { invoke, isTauri } from '@tauri-apps/api/core';

import type { components, operations } from './generated/api';
import { endpoints } from './generated/endpoints';

type Schemas = components['schemas'];
export type User = Schemas['User'];
export type Patient = Schemas['Patient'];
export type PatientPage = Schemas['PatientPage'];
export type DiaryEntry = Schemas['DiaryEntry'];
export type DiaryPage = Schemas['Page_DiaryEntry'];
export type EventPage = Schemas['Page_ObservationEvent'];
export type HistoryCursor = Schemas['HistoryCursor'];
export type EventKind = Schemas['EventKind'];
export type ObservationEvent = Schemas['ObservationEvent'];
export type ProfileChange = Schemas['ProfileChange'];
export type ProfileInput = Schemas['ProfileInput'];
export type ChangesPage = Schemas['ChangesPage'];
export type Invitation = Schemas['Invitation'];
export type Notification = Schemas['Notification'];
export type NotificationPage = Schemas['NotificationPage'];
export type Prescription = Schemas['Prescription'];
export type PrescriptionChange = Schemas['PrescriptionChange'];
export type AdherenceSummary = Schemas['AdherenceSummary'];
export type AdverseEvent = Schemas['AdverseEvent'];
export type Alert = Schemas['Alert'];
export type AlertPage = Schemas['AlertPage'];
export type AlertChange = Schemas['AlertChange'];
export type Appointment = Schemas['Appointment'];
export type AppointmentPage = Schemas['AppointmentPage'];
export type ChatMessage = Schemas['ChatMessage'];
export type ChatPage = Schemas['ChatPage'];
export type DirectoryQuery = operations['list_patients_page']['parameters']['query'];
export const eventKinds = {
  initial_assessment: 'Первичный осмотр',
  consultation: 'Консультация',
  plan_change: 'Изменение плана',
  note: 'Заметка',
  other: 'Другое',
} satisfies Record<EventKind, string>;

type Operation = Exclude<keyof typeof endpoints, 'health' | 'ready'> & keyof operations;
type Parameters<K extends Operation, P extends 'path' | 'query'> = Exclude<
  operations[K]['parameters'][P],
  undefined
>;
type PathOptions<K extends Operation> = [Parameters<K, 'path'>] extends [never]
  ? { params?: never }
  : { params: Parameters<K, 'path'> };
type QueryOptions<K extends Operation> = [Parameters<K, 'query'>] extends [never]
  ? { query?: never }
  : { query?: Parameters<K, 'query'> };
type BodyOptions<K extends Operation> = operations[K] extends {
  requestBody: { content: { 'application/json': infer B } };
}
  ? { body: B }
  : { body?: never };
export type ApiOptions<K extends Operation> = PathOptions<K> & QueryOptions<K> & BodyOptions<K>;
type Successful<R> = {
  [S in keyof R]: `${S & (string | number)}` extends `2${string}`
    ? R[S] extends { content: { 'application/json': infer B } }
      ? B
      : void
    : never;
}[keyof R];
export type ApiResult<K extends Operation> = Successful<operations[K]['responses']>;

export async function api<K extends Operation>(
  operation: K,
  ...args: {} extends ApiOptions<K> ? [options?: ApiOptions<K>] : [options: ApiOptions<K>]
): Promise<ApiResult<K>> {
  const endpoint = endpoints[operation];
  const options = (args[0] ?? {}) as {
    params?: Record<string, string>;
    query?: Record<string, unknown>;
    body?: unknown;
  };
  const path = endpoint.path.replace(/\{([^}]+)\}/g, (_, name: string) => {
    const value = options.params?.[name];
    if (typeof value !== 'string') throw new ApiError(0, 'Не передан параметр пути API');
    return encodeURIComponent(value);
  });
  const query = options.query
    ? Object.fromEntries(
        Object.entries(options.query)
          .filter(([, value]) => value !== undefined && value !== null)
          .map(([key, value]) => [key, String(value)]),
      )
    : undefined;
  // Обе платформы используют один контракт, а нативная сессия остаётся внутри Rust.
  return (await transport(
    path.slice('/api/v1'.length),
    endpoint.method,
    options.body,
    query,
  )) as ApiResult<K>;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public code?: string,
  ) {
    super(message);
  }
}

async function transport(
  path: string,
  method = 'GET',
  body?: unknown,
  query?: Record<string, string>,
): Promise<unknown> {
  let status: number;
  let payload: unknown;
  try {
    if (isTauri()) {
      // Сессия остаётся в памяти Rust-клиента и не попадает в localStorage или WebView.
      const result = await invoke<{ status: number; body: unknown }>('api_request', {
        path,
        method,
        body: body ?? null,
        query: query ?? null,
      });
      status = result.status;
      payload = result.body;
    } else {
      const parameters = query ? `?${new URLSearchParams(query)}` : '';
      const response = await fetch(
        `${import.meta.env.VITE_API_URL ?? ''}/api/v1${path}${parameters}`,
        {
          method,
          credentials: 'include',
          signal: AbortSignal.timeout(15000),
          headers: { 'Content-Type': 'application/json', 'X-Synapse-Request': '1' },
          body: body === undefined ? undefined : JSON.stringify(body),
        },
      );
      status = response.status;
      const text = await response.text();
      try {
        payload = text ? JSON.parse(text) : null;
      } catch {
        payload = null;
      }
    }
  } catch (cause) {
    if (
      cause &&
      typeof cause === 'object' &&
      'code' in cause &&
      'message' in cause &&
      typeof cause.code === 'string' &&
      typeof cause.message === 'string'
    ) {
      throw new ApiError(0, cause.message, cause.code);
    }
    throw new ApiError(
      0,
      'Не удалось связаться с сервером. Проверьте подключение и повторите запрос.',
    );
  }
  if (status < 200 || status >= 300) {
    const error = payload as { error?: { message?: string; code?: string } } | null;
    throw new ApiError(
      status,
      error?.error?.message ?? `Не удалось выполнить запрос (${status})`,
      error?.error?.code,
    );
  }
  return payload;
}

export const date = (value: string, time = false) =>
  new Intl.DateTimeFormat('ru-RU', {
    day: 'numeric',
    month: 'short',
    ...(time ? ({ hour: '2-digit', minute: '2-digit' } as const) : {}),
  }).format(new Date(value.length === 10 ? `${value}T12:00:00` : value));
export const initials = (name: string) =>
  name
    .split(' ')
    .slice(0, 2)
    .map((p) => p[0])
    .join('');
export const statusLabel = (status: Patient['status']) =>
  ({ active: 'Активный', observation: 'Наблюдение', archived: 'В архиве' })[status];
export function localDateTime() {
  const now = new Date();
  return new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
}

export function eventDateTime(value: string) {
  const parsed = new Date(value);
  if (!Number.isFinite(parsed.getTime())) throw new Error('Проверьте дату и время события');
  // Смещение относится к выбранной дате, включая историческое летнее время.
  const minutes = -parsed.getTimezoneOffset();
  const sign = minutes < 0 ? '-' : '+';
  const hours = String(Math.floor(Math.abs(minutes) / 60)).padStart(2, '0');
  const remainder = String(Math.abs(minutes) % 60).padStart(2, '0');
  return `${value.length === 16 ? `${value}:00` : value}${sign}${hours}:${remainder}`;
}

export const eventDateLabel = (value: string) =>
  new Intl.DateTimeFormat('ru-RU', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
