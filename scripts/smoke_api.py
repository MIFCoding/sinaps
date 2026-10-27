"""Интеграционные проверки запущенного локального API только на синтетических аккаунтах."""
import concurrent.futures
import datetime as dt
import http.cookiejar
import json
import os
import urllib.error
import urllib.request
import uuid

BASE = os.environ.get("SYNAPSE_TEST_API", "http://127.0.0.1:3000")
PATIENT = "00000000-0000-0000-0000-000000000002"
OTHER = "00000000-0000-0000-0000-000000000006"


class Client:
    def __init__(self):
        self.cookies = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.cookies))

    def request(self, path, method="GET", data=None, expected=200, headers=None):
        actual_headers = {"Content-Type": "application/json", "X-Synapse-Request": "1"}
        if headers is not None:
            actual_headers = headers
        req = urllib.request.Request(BASE + path, method=method, headers=actual_headers,
                                     data=json.dumps(data).encode() if data is not None else None)
        try:
            response = self.opener.open(req, timeout=15)
        except urllib.error.HTTPError as exc:
            response = exc
        status = response.code
        raw = response.read()
        assert status == expected, f"{method} {path}: expected {expected}, got {status}: {raw[:250]!r}"
        if not raw:
            return None
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            # Ошибки разбора JSON в Axum могут возвращаться текстом, как описано в контракте.
            assert status >= 400, 'Успешный ответ API должен содержать JSON'
            return raw.decode('utf-8')

    def login(self, email):
        return self.request("/api/v1/auth/login", "POST", {"email": email, "password": "Synapse2026!"})


def main():
    anonymous = Client()
    anonymous.request("/health")
    anonymous.request("/ready")
    anonymous.request("/api/v1/patients", expected=401)
    anonymous.request("/api/v1/auth/login", "POST", {"email": "missing@synapse.demo", "password": "bad"}, expected=401)
    anonymous.request("/api/v1/auth/login", "POST", {}, expected=403, headers={"Content-Type": "application/json"})
    anonymous.request("/api/v1/auth/logout", "POST", expected=403, headers={"X-Synapse-Request": "1", "Origin": "https://untrusted.invalid"})

    doctor, patient, outsider, other_patient = Client(), Client(), Client(), Client()
    assert doctor.login("doctor@synapse.demo")["role"] == "doctor"
    assert patient.login("patient@synapse.demo")["role"] == "patient"
    outsider.login("other-doctor@synapse.demo")
    other_patient.login("other-patient@synapse.demo")
    assert {"00000000-0000-0000-0000-000000000002", "00000000-0000-0000-0000-000000000003", "00000000-0000-0000-0000-000000000004"}.issubset({p["id"] for p in doctor.request("/api/v1/patients")})
    assert [p["id"] for p in patient.request("/api/v1/patients")] == [PATIENT]
    for client, target in [(doctor, OTHER), (outsider, PATIENT), (patient, OTHER), (other_patient, PATIENT)]:
        for suffix in ["", "/diary", "/events"]:
            client.request(f"/api/v1/patients/{target}{suffix}", expected=403)

    entry = {"id": str(uuid.uuid4()), "measured_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds"),
             "mood": 0, "anxiety": 10, "sleep_hours": 0.0, "comment": "API integration: zero is a valid measurement"}
    endpoint = f"/api/v1/patients/{PATIENT}/diary"
    doctor.request(endpoint, "POST", entry, expected=403)
    other_patient.request(endpoint, "POST", entry, expected=403)
    patient.request(endpoint, "POST", {**entry, "mood": 11}, expected=422)
    patient.request(endpoint, "POST", {**entry, "sleep_hours": -1}, expected=422)
    future = (dt.datetime.now(dt.timezone.utc) + dt.timedelta(hours=1)).isoformat()
    patient.request(endpoint, "POST", {**entry, "measured_at": future}, expected=422)
    patient.request(endpoint, "POST", entry, expected=201)
    patient.request(endpoint, "POST", entry, expected=200)
    patient.request(endpoint, "POST", {**entry, "mood": 4}, expected=409)
    records = doctor.request(endpoint)
    assert sum(row["id"] == entry["id"] for row in records) == 1
    assert next(row for row in records if row["id"] == entry["id"])["mood"] == 0

    # Одновременные отправки одного нового идентификатора создают ровно одну запись.
    parallel = {**entry, "id": str(uuid.uuid4()), "comment": "API integration: concurrent retry"}
    def submit():
        # Отдельный клиент для каждого запроса исключает совместное изменение состояния соединения.
        client = Client()
        for cookie in patient.cookies:
            client.cookies.set_cookie(cookie)
        req = urllib.request.Request(BASE + endpoint, method="POST", data=json.dumps(parallel).encode(),
                                     headers={"Content-Type": "application/json", "X-Synapse-Request": "1"})
        with client.opener.open(req, timeout=15) as response:
            return response.status
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(lambda _: submit(), range(2))) == [200, 201]
    assert sum(row["id"] == parallel["id"] for row in doctor.request(endpoint)) == 1

    event_path = f"/api/v1/patients/{PATIENT}/events"
    private = {"id": str(uuid.uuid4()), "title": "Internal integration note", "body": "Not visible to patient", "patient_visible": False}
    patient.request(event_path, "POST", private, expected=403)
    doctor.request(event_path, "POST", private, expected=201)
    doctor.request(event_path, "POST", private, expected=200)
    assert any(e["id"] == private["id"] for e in doctor.request(event_path))
    assert all(e["id"] != private["id"] for e in patient.request(event_path))
    public = {**private, "id": str(uuid.uuid4()), "patient_visible": True, "title": "Shared integration note"}
    doctor.request(event_path, "POST", public, expected=201)
    assert any(e["id"] == public["id"] for e in patient.request(event_path))
    patient.request("/api/v1/auth/logout", "POST", expected=204)
    patient.request("/api/v1/me", expected=401)
    print("PASS: auth, CSRF, ownership, validation, concurrent idempotency, note visibility and logout")
    print("Synthetic integration records were added to the local demo database.")


if __name__ == "__main__":
    main()
