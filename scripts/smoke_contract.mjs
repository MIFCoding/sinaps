import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

const base = process.env.SYNAPSE_TEST_API ?? 'http://127.0.0.1:3000';
const document = JSON.parse(await readFile('contracts/openapi.json', 'utf8'));
const live = await fetch(`${base}/api/v1/openapi.json`).then((response) => response.json());
assert.deepEqual(live, document, 'Работающий сервер отдаёт другой контракт');
const ajv = new Ajv2020({
  strict: false,
  allErrors: true,
  formats: { int16: true, int32: true, int64: true, double: true },
});
addFormats(ajv);
const operations = new Map();
for (const [path, item] of Object.entries(document.paths)) {
  for (const [method, operation] of Object.entries(item)) {
    if (!operation.operationId) continue;
    assert(!operations.has(operation.operationId), 'Повтор operationId');
    operations.set(operation.operationId, { ...operation, method: method.toUpperCase(), path });
    for (const response of Object.values(operation.responses)) {
      for (const content of Object.values(response.content ?? {})) {
        ajv.compile({ components: document.components, ...content.schema });
      }
    }
  }
}
let doctorCookie;
let checks = 0;
async function request(
  name,
  {
    params = {},
    query = {},
    body,
    cookie = doctorCookie,
    expected = 200,
    validateInput = true,
    rawBody,
    contentType = 'application/json',
  } = {},
) {
  const operation = operations.get(name);
  assert(operation, `Неизвестная операция ${name}`);
  let path = operation.path.replace(/\{([^}]+)\}/g, (_, key) => encodeURIComponent(params[key]));
  const search = new URLSearchParams(query).toString();
  if (search) path += `?${search}`;
  if (body && validateInput) {
    const validate = ajv.compile({
      components: document.components,
      ...operation.requestBody.content['application/json'].schema,
    });
    assert(
      validate(body),
      `${name}: тело запроса не соответствует контракту: ${ajv.errorsText(validate.errors)}`,
    );
  }
  const response = await fetch(base + path, {
    method: operation.method,
    headers: {
      'Content-Type': contentType,
      'X-Synapse-Request': '1',
      ...(cookie ? { Cookie: cookie } : {}),
    },
    body: rawBody ?? (body === undefined ? undefined : JSON.stringify(body)),
  });
  assert.equal(response.status, expected, `${name}: неверный HTTP-статус`);
  const raw = await response.text();
  const schemaResponse = operation.responses[String(response.status)];
  assert(schemaResponse, `${name}: статус ${response.status} не описан`);
  let data;
  if (response.status === 204) assert.equal(raw, '', '204 не должен содержать тело');
  else {
    const media = response.headers.get('content-type')?.split(';')[0];
    const schema = schemaResponse.content?.[media]?.schema;
    assert(schema, `${name}: формат ${media} не описан`);
    data = media === 'application/json' ? JSON.parse(raw) : raw;
    const validate = ajv.compile({ components: document.components, ...schema });
    assert(
      validate(data),
      `${name}: ответ не соответствует контракту: ${ajv.errorsText(validate.errors)}`,
    );
  }
  checks++;
  return { data, cookie: response.headers.get('set-cookie')?.split(';')[0] };
}

await request('health');
await request('ready');
await request('me', { expected: 401 });
doctorCookie = (
  await request('login', { body: { email: 'doctor@synapse.demo', password: 'Synapse2026!' } })
).cookie;
assert(doctorCookie);
await request('me');
await request('list_patients');
await request('list_patients_page', { query: { limit: 2 } });
await request('list_patients_page', { query: { limit: 'ошибка' }, expected: 400 });

const id = randomUUID(),
  params = { id };
const { data: created } = await request('create_patient', {
  expected: 201,
  body: {
    id,
    name: `Контракт ${id.slice(0, 8)}`,
    email: `contract-${id}@synapse.demo`,
    birth_date: '1990-01-01',
  },
});
await mkdir('tmp', { recursive: true });
const fixture = 'tmp/contract-api-fixtures.json';
const previous = JSON.parse(await readFile(fixture, 'utf8').catch(() => '[]'));
await writeFile(fixture, JSON.stringify([...previous, id]));
const { data: profile } = await request('get_patient', { params });
const validatePatient = ajv.compile({
  components: document.components,
  $ref: '#/components/schemas/Patient',
});
assert(
  !validatePatient({ ...profile, status: 'unknown' }),
  'Схема должна отклонять неизвестный статус',
);
const withoutNullable = { ...profile };
delete withoutNullable.latest_mood;
assert(!validatePatient(withoutNullable), 'Nullable-поле ответа должно оставаться обязательным');
const patch = Object.fromEntries(
  [
    'version',
    'name',
    'birth_date',
    'observation_started',
    'status',
    'medical_summary',
    'allergies',
  ].map((key) => [key, created[key]]),
);
await request('update_patient', {
  params,
  body: {
    ...patch,
    medical_summary: 'Синтетическая проверка контракта',
    medical_history: {
      complaints: 'Синтетические жалобы',
      present_illness: 'История состояния',
      life_history: '',
      comorbidities_status: 'present',
      comorbidities_details: 'Синтетические сведения',
    },
  },
});
await request('update_patient', { params, body: patch, expected: 409 });
await request('list_profile_changes', { params });
await request('list_profile_changes_page', { params });
const { data: invitation } = await request('create_invitation', { params });
await request('invitation_preview', { params: { code: invitation.code }, cookie: null });
await request('invitation_preview', { params: { code: 'invalid' }, cookie: null, expected: 404 });
const patientCookie = (
  await request('activate', {
    body: { code: invitation.code, password: 'Контракт2026!' },
    cookie: null,
  })
).cookie;
assert(patientCookie);
const registrationBody = {
  email: `contract-registration-${randomUUID()}@synapse.demo`,
  first_name: 'Контракт',
  last_name: 'Регистрации',
  password: 'Регистрация2026!',
  password_confirmation: 'Регистрация2026!',
  consent: true,
};
const { data: registered } = await request('register', {
  body: registrationBody,
  cookie: null,
  expected: 201,
});
await writeFile(
  fixture,
  JSON.stringify([...JSON.parse(await readFile(fixture, 'utf8')), registered.id]),
);
await request('register', { body: registrationBody, cookie: null, expected: 409 });
await request('register', {
  body: { ...registrationBody, consent: false },
  cookie: null,
  expected: 422,
});
await request('get_patient', { params, cookie: patientCookie });
await request('update_patient', {
  params,
  body: {
    ...patch,
    version: 2,
    medical_history: {
      ...created.medical_history,
      comorbidities_status: 'present',
    },
  },
  expected: 422,
});
await request('update_patient', {
  params,
  body: {
    ...patch,
    version: 2,
    medical_history: {
      ...created.medical_history,
      comorbidities_status: 'invalid',
    },
  },
  expected: 422,
  validateInput: false,
});
await request('list_profile_changes_page', { params, cookie: patientCookie, expected: 403 });
const entry = {
  id: randomUUID(),
  measured_at: new Date().toISOString(),
  mood: 5,
  anxiety: 4,
  sleep_hours: 8,
  comment: 'Проверка схемы дневника',
};
await request('save_diary', { params, cookie: patientCookie, expected: 201, body: entry });
await request('save_diary', { params, cookie: patientCookie, body: entry });
await request('save_diary', {
  params,
  cookie: patientCookie,
  expected: 422,
  validateInput: false,
  body: { ...entry, id: randomUUID(), mood: 11 },
});
await request('save_diary', {
  params,
  cookie: patientCookie,
  expected: 422,
  validateInput: false,
  body: { ...entry, mood: 'не число' },
});
await request('list_diary', { params });
await request('list_diary_page', { params });
await request('get_diary_entry', { params: { id, entry_id: entry.id } });
await request('get_diary_entry', { params: { id, entry_id: randomUUID() }, expected: 404 });
const event = {
  id: randomUUID(),
  title: 'Проверка схемы события',
  body: 'Синтетическая запись',
  patient_visible: true,
};
await request('save_event', { params, body: event, expected: 201 });
await request('get_event', { params: { id, event_id: event.id } });
await request('list_events', { params });
await request('list_events_page', { params });
const { data: notifications } = await request('list_notifications');
const own = notifications.items.find((item) => item.patient_id === id);
assert(own);
await request('read_notification', { params: { id: own.id } });
await request('read_all_notifications', { cookie: patientCookie, expected: 204 });
await request('create_patient', { contentType: 'text/plain', rawBody: 'не JSON', expected: 415 });
await request('create_patient', { rawBody: '{', expected: 400 });
await request('create_patient', { rawBody: 'x'.repeat(300_000), expected: 413 });
const { data: clinicalFindings } = await request('clinical_findings', {
  query: { search: 'F41.1', limit: 50 },
});
await request('clinical_codes', { query: { search: 'F41.1' } });
await request('clinical_source', { params: { code: 'F41.1' } });
const { data: drugList } = await request('clinical_drugs');
const drugReference = drugList.find((item) => item.drug_code === 'aviandr')?.spec;
assert(drugReference);
await request('alert_rules');
await request('alert_rules', { cookie: patientCookie, expected: 403 });
await request('clinical_findings', { cookie: patientCookie, expected: 403 });
await request('clinical_drugs', { cookie: patientCookie, expected: 403 });
const finding = clinicalFindings.items.find((item) => item.source_code === 'F41.1');
assert(finding);
const diagnosisBody = {
  id: randomUUID(),
  version: 1,
  catalog_version: clinicalFindings.version,
  code: 'F41.1',
  finding_ids: [finding.id],
  diagnosed_on: new Date().toISOString().slice(0, 10),
  status: 'active',
  note: 'Проверка клинической схемы',
};
await request('create_diagnosis', { params, body: diagnosisBody, expected: 201 });
await request('patient_diagnoses', { params, cookie: patientCookie });
await request('patient_diagnosis', {
  params: { id, diagnosis_id: diagnosisBody.id },
  cookie: patientCookie,
});
await request('update_diagnosis', {
  params: { id, diagnosis_id: diagnosisBody.id },
  body: { ...diagnosisBody, note: 'Новая версия клинической схемы' },
});
await request('diagnosis_changes', { params: { id, diagnosis_id: diagnosisBody.id } });
await request('diagnosis_changes', {
  params: { id, diagnosis_id: diagnosisBody.id },
  cookie: patientCookie,
  expected: 403,
});
const { data: questionnaire } = await request('questionnaire_definition');
await request('questionnaire_definition', { cookie: patientCookie, expected: 403 });
const assignmentBody = {
  id: randomUUID(),
  scale_version: questionnaire.version,
  note: 'Проверка опросника по схеме',
};
await request('assign_questionnaire', { params, body: assignmentBody, expected: 201 });
await request('list_questionnaires', { params, cookie: patientCookie });
const assessmentParams = { id, assignment_id: assignmentBody.id };
await request('get_questionnaire', { params: assessmentParams, cookie: patientCookie });
await request('questionnaire_changes', { params: assessmentParams });
await request('submit_questionnaire', {
  params: assessmentParams,
  cookie: patientCookie,
  body: { version: 1, scale_version: questionnaire.version, answers: [0, 1, 2, 3, 0, 1, 2] },
});
await request('questionnaire_changes', { params: assessmentParams });
await request('questionnaire_changes', {
  params: assessmentParams,
  cookie: patientCookie,
  expected: 403,
});
const cancelledBody = { ...assignmentBody, id: randomUUID() };
await request('assign_questionnaire', { params, body: cancelledBody, expected: 201 });
await request('cancel_questionnaire', {
  params: { id, assignment_id: cancelledBody.id },
  body: { version: 1 },
});
const prescriptionBody = {
  id: randomUUID(),
  version: 1,
  diagnosis_id: diagnosisBody.id,
  drug_code: 'aviandr',
  source_sha256: drugReference.source_sha256,
  dosage: 'Синтетическая проверка контракта',
  intake_times: ['09:00'],
  starts_on: new Date().toISOString().slice(0, 10),
  ends_on: null,
  instructions: 'Только проверка схемы API.',
};
await request('create_prescription', { params, body: prescriptionBody, expected: 201 });
await request('list_prescriptions', { params, cookie: patientCookie });
const prescriptionParams = { id, prescription_id: prescriptionBody.id };
await request('update_prescription', {
  params: prescriptionParams,
  body: { ...prescriptionBody, dosage: 'Обновлённая проверка контракта' },
});
await request('publish_prescription', { params: prescriptionParams, body: { version: 2 } });
await request('list_prescriptions', { params, cookie: patientCookie });
await request('record_adherence', {
  params: prescriptionParams,
  cookie: patientCookie,
  expected: 201,
  body: {
    id: randomUUID(),
    scheduled_on: prescriptionBody.starts_on,
    scheduled_time: '09:00',
    status: 'taken',
    reason: '',
  },
});
await request('prescription_adherence', {
  params: prescriptionParams,
  query: { from: prescriptionBody.starts_on, to: prescriptionBody.starts_on },
});
await request('prescription_changes', { params: prescriptionParams });
const adverseBody = {
  id: randomUUID(),
  prescription_id: prescriptionBody.id,
  occurred_at: new Date().toISOString(),
  severity: 'mild',
  effect_items: [drugReference.effects[0].items[0]],
  note: 'Синтетическая проверка записи врача.',
};
await request('record_adverse_effects', {
  params,
  expected: 201,
  body: adverseBody,
});
await request('record_adverse_effects', {
  params,
  expected: 200,
  body: adverseBody,
});
await request('record_adverse_effects', {
  params,
  cookie: patientCookie,
  expected: 403,
  body: { ...adverseBody, id: randomUUID() },
});
await request('list_adverse_events', { params, cookie: patientCookie });
const criticalBody = {
  ...adverseBody,
  id: randomUUID(),
  severity: 'severe',
  note: 'Синтетическая проверка отсутствия автоматического алерта.',
};
await request('record_adverse_effects', {
  params,
  expected: 201,
  body: criticalBody,
});
const { data: alertPage } = await request('list_patient_alerts', {
  params,
  cookie: patientCookie,
});
assert(!alertPage.items.some((item) => item.source_id === criticalBody.id));
await request('cancel_prescription', { params: prescriptionParams, body: { version: 3 } });
await request('logout', { cookie: patientCookie, expected: 204 });
await request('logout', { expected: 204 });
console.log(
  `ПРОЙДЕНО: ${checks} HTTP-ответов проверены по OpenAPI; схемы, nullable, ошибки JSON/текст и контракт сервера совпадают.`,
);
