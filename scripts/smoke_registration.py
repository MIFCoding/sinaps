"""Регистрация по макету, приглашение существующего аккаунта и основные сведения."""
import json
from pathlib import Path
import uuid

from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    body = {'email': f'design-api-{uuid.uuid4()}@synapse.demo', 'first_name': 'Пациент',
            'last_name': 'Макета', 'password': 'Дизайн2026!', 'password_confirmation': 'Дизайн2026!', 'consent': True}
    fixture = Path('tmp/registration-api-fixtures.json')

    def remember(identity):
        fixture.parent.mkdir(exist_ok=True)
        previous = json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
        fixture.write_text(json.dumps([*previous, identity]), encoding='utf-8')

    patient.request('/api/v1/auth/register', 'POST', {**body, 'consent': False}, expected=422)
    patient.request('/api/v1/auth/register', 'POST', {**body, 'password_confirmation': 'ДругойПароль'}, expected=422)
    user = patient.request('/api/v1/auth/register', 'POST', {**body, 'role': 'doctor'}, expected=201)
    remember(user['id'])
    assert user['role'] == 'patient' and user['name'] == 'Пациент Макета'
    assert patient.request('/api/v1/patients') == []
    patient.request('/api/v1/auth/register', 'POST', body, expected=409)
    profile_body = {'id': str(uuid.uuid4()), 'name': user['name'], 'email': body['email'], 'birth_date': '1990-01-01'}
    profile = doctor.request('/api/v1/patients', 'POST', profile_body, expected=201)
    identity = profile['id']
    assert identity == user['id'] and not profile['account_activated']
    assert doctor.request('/api/v1/patients', 'POST', profile_body)['id'] == identity
    outsider.request('/api/v1/patients', 'POST', {**profile_body, 'id': str(uuid.uuid4())}, expected=409)
    path = f'/api/v1/patients/{identity}'
    patient.request(path, expected=403)
    assert patient.request('/api/v1/patients/page')['items'] == []
    doctor.request(path + '/events', 'POST', {'id': str(uuid.uuid4()), 'title': 'До принятия приглашения', 'body': 'Синтетические сведения', 'patient_visible': True}, expected=201)
    assert patient.request('/api/v1/notifications')['items'] == []
    invitation = doctor.request(path + '/invitation', 'POST')
    preview = patient.request('/api/v1/auth/invitation/' + invitation['code'])
    assert preview['existing_account'] and preview['doctor_name'] == 'Иван Сергеевич'
    patient.request('/api/v1/auth/activate', 'POST', {'code': invitation['code'], 'password': 'НеправильныйПароль'}, expected=422)
    patient.request('/api/v1/auth/activate', 'POST', {'code': invitation['code'], 'password': body['password']})
    assert patient.request(path)['account_activated']
    patient.request('/api/v1/auth/invitation/' + invitation['code'], expected=404)
    profile = doctor.request(path)
    fields = ['version', 'name', 'birth_date', 'observation_started', 'status', 'medical_summary', 'allergies', 'medical_history', 'physical_info']
    update = {key: profile[key] for key in fields}
    update['physical_info'] = {'height_cm': 168, 'weight_kg': 60.5, 'blood_group': 'O (I) Rh+'}
    saved = doctor.request(path, 'PATCH', update)
    assert patient.request(path)['physical_info'] == update['physical_info']
    changes = doctor.request(path + '/changes/page')['items']
    assert changes[0]['before_data']['physical_info']['weight_kg'] is None
    assert changes[0]['after_data']['physical_info']['weight_kg'] == 60.5
    legacy = {key: saved[key] for key in fields if key != 'physical_info'}
    assert doctor.request(path, 'PATCH', legacy)['version'] == saved['version']
    for invalid in [{'height_cm': 0}, {'weight_kg': -1}, {'blood_group': 'несуществующая'}]:
        doctor.request(path, 'PATCH', {**legacy, 'physical_info': {**saved['physical_info'], **invalid}}, expected=422)
    patient.request(path, 'PATCH', {**legacy, 'physical_info': saved['physical_info']}, expected=403)
    phone = '+155501' + str(uuid.uuid4().int % 1000000).zfill(6)
    phone_user = Client().request('/api/v1/auth/register', 'POST', {**body, 'email': phone}, expected=201)
    remember(phone_user['id'])
    phone_session = Client()
    assert phone_session.request('/api/v1/auth/login', 'POST', {'email': phone[:3] + ' (' + phone[3:6] + ') ' + phone[6:], 'password': body['password']})['id'] == phone_user['id']
    phone_session.request('/api/v1/auth/invitation/not-a-token', expected=404)
    print('Регистрация: телефон/почта, согласие, роль, приглашение, запреты до принятия, сохранение пароля, рост/вес/кровь и аудит пройдены.')


if __name__ == '__main__':
    main()
