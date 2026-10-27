"""Проверка анамнеза, неизвестных состояний, совместимости и конкурентного аудита."""
import concurrent.futures
import copy
import json
from pathlib import Path
import uuid

from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    identity = str(uuid.uuid4())
    path = f'/api/v1/patients/{identity}'
    profile = doctor.request('/api/v1/patients', 'POST', {
        'id': identity, 'name': 'Проверка анамнеза',
        'email': f'anamnesis-api-{identity}@synapse.demo', 'birth_date': '1990-01-01',
    }, expected=201)
    fixtures = Path('tmp/medical-history-api-fixtures.json')
    fixtures.parent.mkdir(exist_ok=True)
    previous = json.loads(fixtures.read_text(encoding='utf-8')) if fixtures.exists() else []
    fixtures.write_text(json.dumps([*previous, identity]), encoding='utf-8')
    code = doctor.request(path + '/invitation', 'POST')['code']
    patient.request('/api/v1/auth/activate', 'POST', {'code': code, 'password': 'Анамнез2026!'})
    empty = profile['medical_history']
    assert empty == {'complaints': '', 'present_illness': '', 'life_history': '',
                     'comorbidities_status': 'unknown', 'comorbidities_details': ''}
    fields = ('version', 'name', 'birth_date', 'observation_started', 'status',
              'medical_summary', 'allergies', 'medical_history')
    update = {key: profile[key] for key in fields}
    history = {'complaints': '  Жалобы\nсо слов пациента  ',
               'present_illness': 'Синтетическая история текущего состояния',
               'life_history': '  Синтетический анамнез жизни  ',
               'comorbidities_status': 'present', 'comorbidities_details': 'Сведения из выписки'}
    update['medical_history'] = history
    patient.request(path, 'PATCH', update, expected=403)
    outsider.request(path, 'PATCH', update, expected=403)
    outsider.request(path, expected=403)
    saved = doctor.request(path, 'PATCH', update)
    assert saved['version'] == 2
    assert saved['medical_history']['complaints'] == 'Жалобы\nсо слов пациента'
    assert patient.request(path)['medical_history'] == saved['medical_history']
    changes = doctor.request(path + '/changes/page')['items']
    assert changes[0]['before_data']['medical_history'] == empty
    assert changes[0]['after_data']['medical_history'] == saved['medical_history']
    patient.request(path + '/changes/page', expected=403)
    doctor.request(path, 'PATCH', update, expected=409)
    current = {key: saved[key] for key in fields}
    assert doctor.request(path, 'PATCH', current)['version'] == 2
    # Старый клиент не передаёт анамнез: он не должен очищать сведения или создавать ложную версию.
    legacy = {key: value for key, value in current.items() if key != 'medical_history'}
    assert doctor.request(path, 'PATCH', legacy)['version'] == 2
    legacy['medical_summary'] = 'Изменение старым клиентом'
    saved = doctor.request(path, 'PATCH', legacy)
    assert saved['version'] == 3 and saved['medical_history'] == current['medical_history']
    current = {key: saved[key] for key in fields}
    for invalid in [
        {**history, 'comorbidities_status': 'present', 'comorbidities_details': '   '},
        {**history, 'comorbidities_status': 'none'},
        {**history, 'comorbidities_status': 'unknown'},
        {**history, 'complaints': 'я' * 6001},
        {**history, 'life_history': 'текст\u0000'},
    ]:
        doctor.request(path, 'PATCH', {**current, 'medical_history': invalid}, expected=422)
    assert doctor.request(path)['version'] == 3
    # Два изменения одной версии не могут потерять анамнез или историю автора.
    def patch(index):
        client = Client()
        for cookie in doctor.cookies:
            client.cookies.set_cookie(cookie)
        body = copy.deepcopy(current)
        body['medical_history']['complaints'] = f'Параллельное изменение {index}'
        try:
            client.request(path, 'PATCH', body)
            return 200
        except AssertionError as error:
            if 'got 409' in str(error):
                return 409
            raise
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(patch, range(2))) == [200, 409]
    saved = doctor.request(path)
    assert saved['version'] == 4
    for state in ['none', 'unknown']:
        body = {key: saved[key] for key in fields}
        body['medical_history'] = {**saved['medical_history'],
                                  'comorbidities_status': state, 'comorbidities_details': ''}
        saved = doctor.request(path, 'PATCH', body)
        assert patient.request(path)['medical_history']['comorbidities_status'] == state
    assert saved['version'] == 6
    assert len(doctor.request(path + '/changes/page')['items']) == 5
    # Допустимый многобайтный анамнез превышает прежний общий лимит 32 КиБ.
    large = {key: saved[key] for key in fields}
    large['medical_summary'] = 'я' * 6000
    large['allergies'] = 'я' * 2000
    large['medical_history'] = {key: 'я' * 6000 for key in
                                ['complaints', 'present_illness', 'life_history', 'comorbidities_details']}
    large['medical_history']['comorbidities_status'] = 'present'
    saved = doctor.request(path, 'PATCH', large)
    assert saved['version'] == 7 and len(saved['medical_history']['life_history']) == 6000
    print('Анамнез: права, три состояния сведений, валидация, старый клиент, версии, конкуренция и аудит пройдены.')


if __name__ == '__main__':
    main()
