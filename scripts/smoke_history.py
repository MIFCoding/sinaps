"""Курсор истории, одинаковые даты, приватность и фильтр вне первой страницы."""
import json
from pathlib import Path
from urllib.parse import urlencode
from uuid import uuid4
from smoke_api import Client


def main():
    doctor, patient, outsider = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    identity = str(uuid4())
    fixture = Path('tmp/history-api-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    previous = json.loads(fixture.read_text()) if fixture.exists() else []
    doctor.request('/api/v1/patients', 'POST', {'id': identity, 'name': 'История ' + identity[:8], 'email': f'history-api-{identity}@synapse.demo', 'birth_date': '1990-01-01'}, expected=201)
    fixture.write_text(json.dumps([*previous, identity]))
    invitation = doctor.request(f'/api/v1/patients/{identity}/invitation', 'POST')
    patient.request('/api/v1/auth/activate', 'POST', {'code': invitation['code'], 'password': 'История2026!'})
    base = f'/api/v1/patients/{identity}'
    initial_ids = {item['id'] for item in doctor.request(base + '/events')}
    diary_ids, visible_ids, event_ids = set(), set(initial_ids), set(initial_ids)
    for index in range(23):
        entry_id, event_id = str(uuid4()), str(uuid4())
        diary_ids.add(entry_id)
        event_ids.add(event_id)
        visible = index % 3 == 0
        if visible:
            visible_ids.add(event_id)
        patient.request(base + '/diary', 'POST', {'id': entry_id, 'measured_at': '2020-01-01T12:00:00Z', 'mood': 5, 'anxiety': 4, 'sleep_hours': 8, 'comment': f'История {index}'}, expected=201)
        doctor.request(base + '/events', 'POST', {'id': event_id, 'title': f'Событие {index}', 'body': 'Синтетическая проверка', 'patient_visible': visible, 'kind': 'consultation' if index == 0 else 'note', 'occurred_at': '2020-01-01T12:00:00Z'}, expected=201)

    def collect(client, suffix, extra=None):
        query = {'limit': '2', **(extra or {})}
        rows = []
        while True:
            page = client.request(base + suffix + '/page?' + urlencode(query))
            rows.extend(page['items'])
            if not page['next_cursor']:
                break
            query.update(page['next_cursor'])
        ids = [row['id'] for row in rows]
        assert len(ids) == len(set(ids))
        return set(ids)

    assert collect(doctor, '/diary') == diary_ids
    assert collect(patient, '/diary') == diary_ids
    assert collect(doctor, '/events') == event_ids
    assert collect(patient, '/events') == visible_ids
    filtered = doctor.request(base + '/events/page?kind=consultation&limit=1')
    assert len(filtered['items']) == 1 and filtered['items'][0]['title'] == 'Событие 0' and filtered['next_cursor'] is None
    # Новая запись не сдвигает следующую страницу старого курсора.
    first = doctor.request(base + '/events/page?limit=2')
    before = doctor.request(base + '/events/page?' + urlencode({'limit': 2, **first['next_cursor']}))
    doctor.request(base + '/events', 'POST', {'id': str(uuid4()), 'title': 'Новое событие', 'body': '', 'patient_visible': False}, expected=201)
    after = doctor.request(base + '/events/page?' + urlencode({'limit': 2, **first['next_cursor']}))
    assert before == after
    for suffix in ['/diary/page', '/events/page']:
        outsider.request(base + suffix, expected=403)
        Client().request(base + suffix, expected=401)
        for query in ['limit=0', 'limit=366', 'before=2020-01-01T00:00:00Z', 'before_id=' + str(uuid4())]:
            doctor.request(base + suffix + '?' + query, expected=422)
        doctor.request(base + suffix + '?before=not-a-date', expected=400)
    doctor.request(base + '/events/page?kind=unknown', expected=422)
    doctor.request(base + '/events/page?limit=51', expected=422)
    doctor.request(base + '/diary/page?kind=note', expected=422)
    print('ПРОЙДЕНО: история, курсор, одинаковые даты, видимость, фильтр, права и добавление между страницами')


if __name__ == '__main__':
    main()
