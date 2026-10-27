"""Курсор журнала версий, целостность страниц и доступ только ведущему врачу."""
import json
from pathlib import Path
from urllib.parse import urlencode
from uuid import uuid4
from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    identity = str(uuid4())
    base = f'/api/v1/patients/{identity}'
    fixture = Path('tmp/profile-changes-api-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    previous = json.loads(fixture.read_text()) if fixture.exists() else []
    profile = doctor.request('/api/v1/patients', 'POST', {'id': identity, 'name': 'Версии ' + identity[:8], 'email': f'changes-api-{identity}@synapse.demo', 'birth_date': '1990-01-01'}, expected=201)
    fixture.write_text(json.dumps([*previous, identity]))
    assert doctor.request(base + '/changes/page') == {'items': [], 'next_cursor': None}
    keys = ['version', 'name', 'birth_date', 'observation_started', 'status', 'medical_summary', 'allergies']
    for index in range(55):
        profile = doctor.request(base, 'PATCH', {key: profile[key] for key in keys} | {'medical_summary': f'Синтетическое изменение {index}'})
    # Отправка неизменившейся карточки не создаёт новую версию.
    unchanged = doctor.request(base, 'PATCH', {key: profile[key] for key in keys})
    assert unchanged['version'] == profile['version']
    first = doctor.request(base + '/changes/page?limit=7')
    collected, page = [], first
    while True:
        collected.extend(page['items'])
        if not page['next_cursor']:
            break
        page = doctor.request(base + '/changes/page?' + urlencode({'limit': 7, **page['next_cursor']}))
    assert len(collected) == len({item['id'] for item in collected}) == 55
    assert [item['version_after'] for item in collected] == list(range(56, 1, -1))
    assert all(item['version_after'] == item['version_before'] + 1 for item in collected)
    assert collected[-1]['after_data']['medical_summary'] == 'Синтетическое изменение 0'
    second_query = {'limit': 7, **first['next_cursor']}
    before = doctor.request(base + '/changes/page?' + urlencode(second_query))
    doctor.request(base, 'PATCH', {key: profile[key] for key in keys} | {'medical_summary': 'Добавлено между страницами'})
    assert doctor.request(base + '/changes/page?' + urlencode(second_query)) == before
    assert len(doctor.request(base + '/changes/page?limit=50')['items']) == 50
    assert doctor.request(base + '/changes/page?before_version=2')['items'] == []
    invitation = doctor.request(base + '/invitation', 'POST')
    patient.request('/api/v1/auth/activate', 'POST', {'code': invitation['code'], 'password': 'Версии2026!'})
    for client, status in [(Client(), 401), (outsider, 403), (patient, 403)]:
        client.request(base + '/changes/page?' + urlencode(second_query), expected=status)
    doctor.request('/api/v1/patients/00000000-0000-0000-0000-000000000006/changes/page', expected=403)
    for query in ['limit=0', 'limit=51', 'before_version=0', 'before_version=-1', 'before_version=1']:
        doctor.request(base + '/changes/page?' + query, expected=422)
    for query in ['limit=text', 'before_version=text', 'before_version=2147483648']:
        doctor.request(base + '/changes/page?' + query, expected=400)
    print('ПРОЙДЕНО: 55 версий, курсор, отсутствие дублей, неизменённая карточка, добавление между страницами и приватность')


if __name__ == '__main__':
    main()
