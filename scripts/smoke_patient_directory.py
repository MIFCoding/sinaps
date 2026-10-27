"""Проверка серверного поиска, курсора и изоляции списка пациентов."""
import json
from pathlib import Path
from urllib.parse import urlencode
import uuid

from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    patient.login('patient@synapse.demo')
    Client().request('/api/v1/patients/page', expected=401)
    tag = uuid.uuid4().hex[:8]
    names = [f'Каталог {tag} Анна', f'Каталог {tag} Анна', f'Каталог {tag} Мария', f'Каталог {tag} 100%_точно', f'Каталог {tag} Завершён']
    fixture = Path('tmp/directory-api-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    existing = json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
    identities = []
    for name in names:
        identity = str(uuid.uuid4())
        doctor.request('/api/v1/patients', 'POST', {'id': identity, 'name': name, 'email': f'directory-{identity}@synapse.demo', 'birth_date': '1990-01-01'}, expected=201)
        identities.append(identity)
        fixture.write_text(json.dumps([*existing, *identities]), encoding='utf-8')
    target = doctor.request('/api/v1/patients/' + identities[-1])
    doctor.request('/api/v1/patients/' + identities[-1], 'PATCH', {key: target[key] for key in ['version', 'name', 'birth_date', 'observation_started', 'medical_summary', 'allergies']} | {'status': 'archived'})
    parameters = {'search': f'каталог {tag}', 'limit': 2}
    page = doctor.request('/api/v1/patients/page?' + urlencode(parameters))
    assert page['matching_count'] == 5 and len(page['items']) == 2
    collected = list(page['items'])
    while page['next_cursor']:
        cursor = page['next_cursor']
        page = doctor.request('/api/v1/patients/page?' + urlencode({**parameters, 'before_name': cursor['name'], 'before_id': cursor['id']}))
        collected.extend(page['items'])
    assert {item['id'] for item in collected} == set(identities) and len(collected) == 5
    archived = doctor.request('/api/v1/patients/page?' + urlencode({'search': tag, 'status': 'archived'}))
    assert archived['matching_count'] == 1 and archived['items'][0]['id'] == identities[-1]
    literal = doctor.request('/api/v1/patients/page?' + urlencode({'search': f'{tag} 100%_'}))
    assert literal['matching_count'] == 1 and literal['items'][0]['id'] == identities[3]
    assert outsider.request('/api/v1/patients/page?' + urlencode({'search': tag}))['matching_count'] == 0
    own = patient.request('/api/v1/patients/page')
    assert own['total_count'] == 1 and own['items'][0]['email'] == 'patient@synapse.demo'
    assert patient.request('/api/v1/patients/page?' + urlencode({'search': tag}))['matching_count'] == 0
    for query in ['limit=0', 'limit=51', 'status=unknown', 'before_name=Анна', 'before_id=' + str(uuid.uuid4()), 'search=' + 'я'*121, 'search=\x00']:
        doctor.request('/api/v1/patients/page?' + urlencode(dict([query.split('=',1)])), expected=422)
    print('ПРОЙДЕНО: серверный поиск, буквальные символы, статусы, одинаковые имена, курсор и изоляция пациентов')


if __name__ == '__main__':
    main()
