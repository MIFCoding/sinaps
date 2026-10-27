"""Сквозные проверки подключения пациента и конкурентного редактирования карточки."""
import concurrent.futures
import datetime as dt
import json
from pathlib import Path
import uuid

from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login("doctor@synapse.demo")
    outsider.login("other-doctor@synapse.demo")
    patient.login("patient@synapse.demo")
    fixtures = []

    def create():
        identity = str(uuid.uuid4())
        body = {"id": identity, "name": "Пациент проверки подключения",
                "email": f"onboarding-{identity}@synapse.demo", "birth_date": "1995-03-12"}
        profile = doctor.request("/api/v1/patients", "POST", body, expected=201)
        fixtures.append(identity)
        return body, profile

    body, profile = create()
    identity = profile["id"]
    path = f"/api/v1/patients/{identity}"
    assert not profile["account_activated"] and profile["version"] == 1
    assert doctor.request("/api/v1/patients", "POST", body)["id"] == identity
    assert len(doctor.request(path + "/events")) == 1
    doctor.request("/api/v1/patients", "POST", {**body, "id": str(uuid.uuid4())}, expected=409)
    patient.request("/api/v1/patients", "POST", body, expected=403)
    doctor.request("/api/v1/patients", "POST", {**body, "id": str(uuid.uuid4()), "birth_date": "2999-01-01"}, expected=422)
    for suffix in ["", "/changes"]:
        outsider.request(path + suffix, expected=403)
    outsider.request(path + "/invitation", "POST", expected=403)

    previous = doctor.request(path + "/invitation", "POST")
    invitation = doctor.request(path + "/invitation", "POST")
    assert previous["code"] != invitation["code"]
    anonymous = Client()
    anonymous.request("/api/v1/auth/login", "POST", {"email": body["email"], "password": "Synapse2026!"}, expected=401)
    anonymous.request("/api/v1/auth/activate", "POST", {"code": previous["code"], "password": "ПарольПроверки2026!"}, expected=422)
    anonymous.request("/api/v1/auth/activate", "POST", {"code": invitation["code"], "password": "123"}, expected=422)
    activated = anonymous.request("/api/v1/auth/activate", "POST", {"code": invitation["code"], "password": "ПарольПроверки2026!"})
    assert activated["id"] == identity and activated["role"] == "patient"
    assert anonymous.request("/api/v1/me")["id"] == identity
    assert doctor.request(path)["account_activated"]
    doctor.request(path + "/invitation", "POST", expected=409)
    Client().request("/api/v1/auth/activate", "POST", {"code": invitation["code"], "password": "ДругойПароль2026!"}, expected=422)

    entry = {"id": str(uuid.uuid4()), "measured_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds"),
             "mood": 6, "anxiety": 3, "sleep_hours": 7.5, "comment": "Проверка дневника после подключения"}
    anonymous.request(path + "/diary", "POST", entry, expected=201)
    assert doctor.request(path + "/diary")[0]["id"] == entry["id"]
    anonymous.request("/api/v1/patients/00000000-0000-0000-0000-000000000002", expected=403)

    fields = ("version", "name", "birth_date", "observation_started", "status", "medical_summary", "allergies")
    update = {key: profile[key] for key in fields}
    update.update(medical_summary="Первое изменение карточки", status="observation", allergies="Не выявлено")
    anonymous.request(path, "PATCH", update, expected=403)
    outsider.request(path, "PATCH", update, expected=403)
    saved = doctor.request(path, "PATCH", update)
    assert saved["version"] == 2 and saved["medical_summary"] == update["medical_summary"]
    doctor.request(path, "PATCH", {**update, "medical_summary": "Устаревшая запись"}, expected=409)
    changes = doctor.request(path + "/changes")
    assert len(changes) == 1 and changes[0]["before_data"]["allergies"] is None
    assert changes[0]["after_data"]["allergies"] == "Не выявлено"
    anonymous.request(path + "/changes", expected=403)

    # Два одновременных изменения одной версии: только одно должно быть зафиксировано.
    current = {key: saved[key] for key in fields}
    def patch(index):
        client = Client()
        for cookie in doctor.cookies:
            client.cookies.set_cookie(cookie)
        request = {**current, "medical_summary": f"Конкурентное изменение {index}"}
        try:
            client.request(path, "PATCH", request)
            return 200
        except AssertionError as error:
            if "got 409" in str(error):
                return 409
            raise
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(patch, range(2))) == [200, 409]
    assert doctor.request(path)["version"] == 3
    assert len(doctor.request(path + "/changes")) == 2
    fresh = doctor.request(path)
    doctor.request(path, "PATCH", {key: fresh[key] for key in fields})
    assert len(doctor.request(path + "/changes")) == 2

    # Одновременная активация одного кода создаёт одну сессию победившего запроса.
    _, pending = create()
    code = doctor.request(f"/api/v1/patients/{pending['id']}/invitation", "POST")["code"]
    def activate(_):
        try:
            Client().request("/api/v1/auth/activate", "POST", {"code": code, "password": "ПарольПроверки2026!"})
            return 200
        except AssertionError as error:
            if "got 422" in str(error):
                return 422
            raise
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(activate, range(2))) == [200, 422]

    # Сохраняем только идентификаторы собственных фикстур, без паролей и кодов приглашения.
    fixture_path = Path("tmp/onboarding-fixtures.json")
    fixture_path.parent.mkdir(exist_ok=True)
    fixture_path.write_text(json.dumps(fixtures), encoding="utf-8")
    print("ПРОЙДЕНО: создание, отзыв и однократность кода, доступ пациента, версии и аудит карточки")


if __name__ == "__main__":
    main()
