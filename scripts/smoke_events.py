"""Проверки ретроспективных событий на отдельном синтетическом пациенте."""
import concurrent.futures
import datetime as dt
import json
from pathlib import Path
import uuid
import urllib.error
import urllib.request

from smoke_api import BASE, Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    identity = str(uuid.uuid4())
    doctor.request('/api/v1/patients', 'POST', {
        'id': identity, 'name': 'Пациент проверки событий',
        'email': f'events-{identity}@synapse.demo', 'birth_date': '1990-01-01',
    }, expected=201)
    fixture = Path('tmp/event-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    fixtures = json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
    fixture.write_text(json.dumps([*fixtures, identity]), encoding='utf-8')
    base = f'/api/v1/patients/{identity}'
    code = doctor.request(base + '/invitation', 'POST')['code']
    patient.request('/api/v1/auth/activate', 'POST', {'code': code, 'password': 'ПарольСобытия2026!'})
    path = base + '/events'
    event = {'id': str(uuid.uuid4()), 'title': '  Прошлая консультация  ',
             'body': 'Синтетическая запись проверки', 'patient_visible': True,
             'kind': 'consultation', 'source': '  Выписка от 12.03.2020  ',
             'occurred_at': '2020-03-12T09:30:00+03:00'}
    outsider.request(path, expected=403)
    outsider.request(path, 'POST', event, expected=403)
    patient.request(path, 'POST', event, expected=403)
    future = (dt.datetime.now(dt.timezone.utc) + dt.timedelta(hours=1)).isoformat()
    for fields in [{'occurred_at': future}, {'occurred_at': '1989-12-31T12:00:00+03:00'},
                   {'source': 'я' * 501}, {'kind': 'unknown'}, {'title': '   '}]:
        doctor.request(path, 'POST', {**event, **fields}, expected=422)
    doctor.request(path, 'POST', event, expected=201)
    original = next(row for row in doctor.request(path) if row['id'] == event['id'])
    assert original['kind'] == 'consultation'
    assert original['title'] == 'Прошлая консультация'
    assert original['source'] == 'Выписка от 12.03.2020'
    assert dt.datetime.fromisoformat(original['occurred_at']) == dt.datetime.fromisoformat(event['occurred_at'])
    assert dt.datetime.fromisoformat(original['created_at']) > dt.datetime.fromisoformat(original['occurred_at'])
    doctor.request(path, 'POST', event)
    assert next(row for row in doctor.request(path) if row['id'] == event['id']) == original
    for fields in [{'occurred_at': '2020-03-13T09:30:00+03:00'}, {'occurred_at': None},
                   {'kind': 'note'}, {'source': 'Другой источник'}, {'patient_visible': False}]:
        doctor.request(path, 'POST', {**event, **fields}, expected=409)
    doctor.request('/api/v1/patients/00000000-0000-0000-0000-000000000004/events', 'POST', event, expected=409)
    private = {**event, 'id': str(uuid.uuid4()), 'patient_visible': False,
               'source': 'Внутренний источник', 'kind': 'other'}
    doctor.request(path, 'POST', private, expected=201)
    assert all(row['id'] != private['id'] and row['source'] != private['source'] for row in patient.request(path))
    assert any(row['id'] == event['id'] and row['source'] == original['source'] for row in patient.request(path))

    # Старый клиент без даты повторяет запрос, не меняя время первой записи.
    legacy = {'id': str(uuid.uuid4()), 'title': 'Запись старого клиента', 'body': '', 'patient_visible': False}
    doctor.request(path, 'POST', legacy, expected=201)
    doctor.request(path, 'POST', legacy)
    stored = next(row for row in doctor.request(path) if row['id'] == legacy['id'])
    assert stored['kind'] == 'note' and stored['source'] is None
    assert stored['occurred_at'] == stored['created_at']

    # Одинаковые одновременные отправки возвращают 201 и 200, сохраняя одну запись.
    parallel = {**event, 'id': str(uuid.uuid4())}
    def concurrent_status(_):
        client = Client()
        for cookie in doctor.cookies:
            client.cookies.set_cookie(cookie)
        request = urllib.request.Request(BASE + path, method='POST', data=json.dumps(parallel).encode(),
                                         headers={'Content-Type': 'application/json', 'X-Synapse-Request': '1'})
        try:
            response = client.opener.open(request, timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            return response.code
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(concurrent_status, range(2))) == [200, 201]
    rows = doctor.request(path)
    assert sum(row['id'] == parallel['id'] for row in rows) == 1
    ordered = [(dt.datetime.fromisoformat(row['occurred_at']), dt.datetime.fromisoformat(row['created_at']), row['id']) for row in rows]
    assert ordered == sorted(ordered, reverse=True)
    assert dt.datetime.fromisoformat(rows[-1]['occurred_at']).year == 2020
    print('ПРОЙДЕНО: даты, типы, источники, права, приватность, совместимость и конкурентный повтор событий')


if __name__ == '__main__':
    main()
