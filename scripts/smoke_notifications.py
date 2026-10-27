"""Проверка транзакционных уведомлений, прочтения, изоляции и курсорных страниц."""
import datetime as dt
import json
from pathlib import Path
from urllib.parse import urlencode
import uuid

from smoke_api import Client


def main():
    doctor, patient, outsider = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    Client().request('/api/v1/notifications', expected=401)
    identity = str(uuid.uuid4())
    doctor.request('/api/v1/patients', 'POST', {'id': identity, 'name': 'Пациент уведомлений API',
        'email': f'notifications-{identity}@synapse.demo', 'birth_date': '1990-01-01'}, expected=201)
    fixture = Path('tmp/notification-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    previous = json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
    fixture.write_text(json.dumps([*previous, identity]), encoding='utf-8')
    base = f'/api/v1/patients/{identity}'
    code = doctor.request(base + '/invitation', 'POST')['code']
    patient.request('/api/v1/auth/activate', 'POST', {'code': code, 'password': 'ПарольУведомлений2026!'})
    assert patient.request('/api/v1/notifications')['unread_count'] == 0
    entry = {'id': str(uuid.uuid4()), 'measured_at': dt.datetime.now(dt.timezone.utc).isoformat(timespec='milliseconds'),
             'mood': 5, 'anxiety': 2, 'sleep_hours': 8, 'comment': 'Не копировать в уведомление'}
    patient.request(base + '/diary', 'POST', entry, expected=201)
    patient.request(base + '/diary', 'POST', entry)
    assert patient.request(base + '/diary/' + entry['id'])['comment'] == entry['comment']
    assert doctor.request(base + '/diary/' + entry['id'])['id'] == entry['id']
    outsider.request(base + '/diary/' + entry['id'], expected=403)
    doctor.request('/api/v1/patients/00000000-0000-0000-0000-000000000004/diary/' + entry['id'], expected=404)
    found = [n for n in doctor.request('/api/v1/notifications')['items'] if n['related_id'] == entry['id']]
    assert len(found) == 1 and found[0]['kind'] == 'diary_entry'
    assert 'comment' not in found[0] and 'body' not in found[0]
    notification = found[0]
    outsider.request(f"/api/v1/notifications/{notification['id']}/read", 'POST', expected=404)
    patient.request(f"/api/v1/notifications/{notification['id']}/read", 'POST', expected=404)
    first_read = doctor.request(f"/api/v1/notifications/{notification['id']}/read", 'POST')
    assert doctor.request(f"/api/v1/notifications/{notification['id']}/read", 'POST') == first_read
    event = {'id': str(uuid.uuid4()), 'title': 'Проверка публикации', 'body': 'Не копировать в уведомление', 'patient_visible': False}
    doctor.request(base + '/events', 'POST', event, expected=201)
    assert doctor.request(base + '/events/' + event['id'])['body'] == event['body']
    patient.request(base + '/events/' + event['id'], expected=404)
    outsider.request(base + '/events/' + event['id'], expected=403)
    assert patient.request('/api/v1/notifications')['items'] == []
    for index in range(5):
        published = {**event, 'id': str(uuid.uuid4()), 'patient_visible': True, 'title': f'Событие {index}'}
        doctor.request(base + '/events', 'POST', published, expected=201)
        doctor.request(base + '/events', 'POST', published)
        assert patient.request(base + '/events/' + published['id'])['id'] == published['id']
    page = patient.request('/api/v1/notifications?limit=2')
    assert len(page['items']) == 2 and page['unread_count'] == 5
    collected = list(page['items'])
    while page['next_cursor']:
        page = patient.request('/api/v1/notifications?' + urlencode({'limit': 2, **page['next_cursor']}))
        collected.extend(page['items'])
    assert len(collected) == 5 and len({n['id'] for n in collected}) == 5
    assert all(n['kind'] == 'published_event' and n['patient_id'] == identity for n in collected)
    for query in ['limit=0', 'limit=51', 'before=2026-01-01T00:00:00Z', 'before_id=' + str(uuid.uuid4())]:
        patient.request('/api/v1/notifications?' + query, expected=422)
    patient.request('/api/v1/notifications/read-all', 'POST', expected=204)
    assert patient.request('/api/v1/notifications')['unread_count'] == 0
    assert all(n['read_at'] is not None for n in patient.request('/api/v1/notifications')['items'])
    patient.request('/api/v1/notifications/read-all', 'POST', expected=204)
    print('ПРОЙДЕНО: уведомления без дублей, приватность, чужие UUID, курсор и сохранение прочтения')


if __name__ == '__main__':
    main()
