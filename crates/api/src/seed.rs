use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::OnceLock;
use uuid::Uuid;

pub fn hash(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .expect("valid Argon2 parameters")
        .to_string()
}

pub fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash(&Uuid::new_v4().to_string()))
}

pub async fn run(db: &PgPool) -> Result<(), sqlx::Error> {
    let password_hash = tokio::task::spawn_blocking(|| hash("Synapse2026!"))
        .await
        .expect("password hashing task");
    let mut tx = db.begin().await?;
    for (id, email, name, role) in [
        (1, "doctor@synapse.demo", "Иван Сергеевич", "doctor"),
        (2, "patient@synapse.demo", "Анна Петрова", "patient"),
    ] {
        sqlx::query("INSERT INTO users(id,email,name,role,password_hash) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(Uuid::from_u128(id)).bind(email).bind(name).bind(role).bind(&password_hash).execute(&mut *tx).await?;
        sqlx::query("UPDATE users SET email_verified=true WHERE id=$1 AND email=$2 AND role=$3")
            .bind(Uuid::from_u128(id))
            .bind(email)
            .bind(role)
            .execute(&mut *tx)
            .await?;
    }
    for (id, doctor, birth, status) in [
        (2, 1, "1998-04-12", "active"),
    ] {
        sqlx::query("INSERT INTO patient_profiles(user_id,doctor_id,birth_date,status,observation_started,medical_summary) VALUES ($1,$2,$3::text::date,$4,current_date - 14,'Синтетический профиль для демонстрации дневника и истории наблюдения. Клинические рекомендации ещё не подключены.') ON CONFLICT DO NOTHING")
            .bind(Uuid::from_u128(id)).bind(Uuid::from_u128(doctor)).bind(birth).bind(status).execute(&mut *tx).await?;
    }
    let diagnosis_id = Uuid::from_u128(3000);
    let diagnosed_on = Utc::now().date_naive() - chrono::Duration::days(8);
    let diagnosis_findings = json!([
        {
            "id": "ab619fa02ff7afd52615",
            "label": "беспокойство и неспособность к релаксации",
            "source_code": "F41.1",
            "source_page": 54
        },
        {
            "id": "c9b6972783d57076e052",
            "label": "чувство нервозности, «на взводе» или психического напряжения",
            "source_code": "F41.1",
            "source_page": 54
        }
    ]);
    let diagnosis_snapshot = json!({
        "id": diagnosis_id,
        "code": "F41.1",
        "title": "Генерализованное тревожное расстройство",
        "catalog_version": "icd10ps-7608f1efdbb8-r2",
        "diagnosed_on": diagnosed_on.to_string()
    });
    let diagnosis_record = json!({
        "id": diagnosis_id,
        "patient_id": Uuid::from_u128(2),
        "code": "F41.1",
        "title": "Генерализованное тревожное расстройство",
        "catalog_version": "icd10ps-7608f1efdbb8-r2",
        "findings": diagnosis_findings,
        "diagnosed_on": diagnosed_on,
        "status": "active",
        "note": "Синтетическая демонстрационная запись. Не является медицинским заключением.",
        "version": 1,
        "author_name": "Иван Сергеевич",
        "updated_by_name": "Иван Сергеевич",
        "created_at": Utc::now(),
        "updated_at": Utc::now()
    });
    sqlx::query("INSERT INTO clinical_diagnoses(id,patient_id,code,title,catalog_version,findings,diagnosed_on,status,note,version,author_id,updated_by) VALUES($1,$2,'F41.1','Генерализованное тревожное расстройство','icd10ps-7608f1efdbb8-r2',$3,$4,'active','Синтетическая демонстрационная запись. Не является медицинским заключением.',1,$5,$5) ON CONFLICT(id) DO NOTHING")
        .bind(diagnosis_id)
        .bind(Uuid::from_u128(2))
        .bind(sqlx::types::Json(&diagnosis_findings))
        .bind(diagnosed_on)
        .bind(Uuid::from_u128(1))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO clinical_diagnosis_changes(id,diagnosis_id,author_id,version,snapshot) VALUES($1,$2,$3,1,$4) ON CONFLICT(id) DO NOTHING")
        .bind(Uuid::from_u128(3001))
        .bind(diagnosis_id)
        .bind(Uuid::from_u128(1))
        .bind(sqlx::types::Json(&diagnosis_record))
        .execute(&mut *tx)
        .await?;
    // Повторное наполнение не перезаписывает измерения и не восстанавливает изменённые пользователем данные.
    let seeded: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM observation_events WHERE id=$1)")
            .bind(Uuid::from_u128(100))
            .fetch_one(&mut *tx)
            .await?;
    if !seeded {
        for (day, mood, anxiety, sleep) in [
            (13, 3, 8, 4.5),
            (11, 4, 7, 5.0),
            (9, 4, 6, 6.0),
            (7, 5, 6, 5.5),
            (5, 6, 5, 7.0),
            (3, 5, 5, 6.5),
            (1, 7, 4, 7.5),
        ] {
            sqlx::query("INSERT INTO diary_entries(id,patient_id,measured_at,mood,anxiety,sleep_hours,comment) VALUES ($1,$2,date_trunc('day',now()) - make_interval(days => $3) + interval '9 hours',$4,$5,$6,'Демонстрационная запись')")
                .bind(Uuid::from_u128(1000+day as u128)).bind(Uuid::from_u128(2)).bind(day)
                .bind(mood as i16).bind(anxiety as i16).bind(sleep).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO observation_events(id,patient_id,author_id,title,body,patient_visible,occurred_at) VALUES ($1,$2,$3,'Начало наблюдения','Создана демонстрационная история. Здесь будут отображаться события наблюдения и записи врача.',true,now() - interval '14 days')")
            .bind(Uuid::from_u128(100)).bind(Uuid::from_u128(2)).bind(Uuid::from_u128(1)).execute(&mut *tx).await?;
    }
    let prescription_id = Uuid::from_u128(2000);
    let drug_snapshot: Value = serde_json::from_str(include_str!("../../../clinical/aviandr.json"))
        .expect("Проверенная справка Авиандра");
    let therapy_seeded: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM prescriptions WHERE id=$1)")
            .bind(prescription_id)
            .fetch_one(&mut *tx)
            .await?;
    if !therapy_seeded {
        let patient_id = Uuid::from_u128(2);
        let doctor_id = Uuid::from_u128(1);
        let starts_on = Utc::now().date_naive() - chrono::Duration::days(7);
        let timestamp = Utc::now().to_rfc3339();
        let dosage = "1 таблетка 2 раза в сутки";
        let intake_times = json!(["09:00", "21:00"]);
        let instructions = "Демонстрационная схема для стенда. Решение о лечении принимает врач.";
        let snapshot = json!({
            "id": prescription_id,
            "patient_id": patient_id,
            "diagnosis_id": diagnosis_id,
            "diagnosis_snapshot": diagnosis_snapshot,
            "drug_code": "aviandr",
            "drug_snapshot": drug_snapshot,
            "dosage": dosage,
            "intake_times": intake_times.clone(),
            "starts_on": starts_on.to_string(),
            "ends_on": Value::Null,
            "instructions": instructions,
            "status": "active",
            "version": 2,
            "author_name": "Иван Сергеевич",
            "updated_by_name": "Иван Сергеевич",
            "created_at": timestamp.clone(),
            "updated_at": timestamp.clone(),
            "published_at": timestamp,
            "cancelled_at": Value::Null
        });
        sqlx::query("INSERT INTO prescriptions(id,patient_id,author_id,updated_by,diagnosis_id,diagnosis_snapshot,drug_code,drug_snapshot,dosage,intake_times,starts_on,instructions,status,version,published_at) VALUES($1,$2,$3,$3,$4,$5,'aviandr',$6,$7,$8,$9,$10,'active',2,now())")
            .bind(prescription_id)
            .bind(patient_id)
            .bind(doctor_id)
            .bind(diagnosis_id)
            .bind(sqlx::types::Json(&diagnosis_snapshot))
            .bind(sqlx::types::Json(drug_snapshot.clone()))
            .bind(dosage)
            .bind(sqlx::types::Json(intake_times))
            .bind(starts_on)
            .bind(instructions)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO prescription_changes(id,prescription_id,author_id,version,action,snapshot) VALUES($1,$2,$3,2,'published',$4)")
            .bind(Uuid::from_u128(2001))
            .bind(prescription_id)
            .bind(doctor_id)
            .bind(sqlx::types::Json(snapshot))
            .execute(&mut *tx)
            .await?;
    } else {
        // Исправляет только собственную фиксированную демо-запись, не затрагивая пользовательские назначения.
        sqlx::query(
            "UPDATE prescriptions SET drug_snapshot=$1 WHERE id=$2 AND drug_code='aviandr'",
        )
        .bind(sqlx::types::Json(drug_snapshot))
        .bind(prescription_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE prescriptions SET diagnosis_id=$1,diagnosis_snapshot=$2 WHERE id=$3 AND patient_id=$4 AND drug_code='aviandr' AND status='active'")
        .bind(diagnosis_id)
        .bind(sqlx::types::Json(&diagnosis_snapshot))
        .bind(prescription_id)
        .bind(Uuid::from_u128(2))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE prescription_changes SET snapshot=snapshot || jsonb_build_object('diagnosis_id',$1::uuid,'diagnosis_snapshot',$2::jsonb) WHERE id=$3 AND prescription_id=$4")
        .bind(diagnosis_id)
        .bind(sqlx::types::Json(&diagnosis_snapshot))
        .bind(Uuid::from_u128(2001))
        .bind(prescription_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!("Synthetic demo accounts ready; credentials are documented in README.md");
    Ok(())
}
