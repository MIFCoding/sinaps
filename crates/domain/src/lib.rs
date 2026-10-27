use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub mod gad7;

#[derive(utoipa::ToSchema)]
pub enum PatientStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "observation")]
    Observation,
    #[serde(rename = "archived")]
    Archived,
}

impl PatientStatus {
    pub fn valid(value: &str) -> bool {
        matches!(value, "active" | "observation" | "archived")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Doctor,
    Patient,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    InitialAssessment,
    Consultation,
    PlanChange,
    #[default]
    Note,
    Other,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InitialAssessment => "initial_assessment",
            Self::Consultation => "consultation",
            Self::PlanChange => "plan_change",
            Self::Note => "note",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct EventInput {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub patient_visible: bool,
    #[serde(default)]
    pub kind: EventKind,
    pub occurred_at: Option<DateTime<chrono::FixedOffset>>,
    pub source: Option<String>,
}

impl EventInput {
    pub fn normalize(&mut self) {
        self.title = self.title.trim().to_owned();
        self.source = self
            .source
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
    }

    pub fn validate(&self, now: DateTime<Utc>, birth_date: NaiveDate) -> Result<(), &'static str> {
        if self.title.is_empty()
            || self.title.chars().count() > 120
            || self.body.chars().count() > 4000
        {
            return Err("Проверьте заголовок (до 120 символов) и текст (до 4000 символов)");
        }
        if self
            .source
            .as_ref()
            .is_some_and(|s| s.chars().count() > 500)
        {
            return Err("Источник сведений должен содержать не более 500 символов");
        }
        if let Some(occurred_at) = self.occurred_at {
            if occurred_at > now + chrono::Duration::minutes(5) {
                return Err("Дата события не может быть в будущем");
            }
            if occurred_at.date_naive() < birth_date {
                return Err("Дата события не может быть раньше даты рождения пациента");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiaryInput {
    pub id: Uuid,
    pub measured_at: DateTime<Utc>,
    pub mood: i16,
    pub anxiety: i16,
    pub sleep_hours: f64,
    #[serde(default)]
    pub comment: String,
}

impl DiaryInput {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), &'static str> {
        if !(0..=10).contains(&self.mood) || !(0..=10).contains(&self.anxiety) {
            return Err("Настроение и тревога должны быть в диапазоне от 0 до 10");
        }
        if !self.sleep_hours.is_finite() || !(0.0..=24.0).contains(&self.sleep_hours) {
            return Err("Продолжительность сна должна быть от 0 до 24 часов");
        }
        if self.comment.chars().count() > 2000 {
            return Err("Комментарий должен содержать не более 2000 символов");
        }
        if self.measured_at > now + chrono::Duration::minutes(5) {
            return Err("Время измерения не может быть в будущем");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct CreatePatientInput {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub birth_date: NaiveDate,
}

impl CreatePatientInput {
    pub fn normalize(&mut self) {
        self.name = self.name.trim().to_owned();
        self.email = normalize_login(&self.email);
    }

    pub fn validate(&self, today: NaiveDate) -> Result<(), &'static str> {
        validate_identity(&self.name, self.birth_date, today)?;
        if !valid_login(&self.email) {
            return Err("Укажите почту или телефон в международном формате, например +79991234567");
        }
        Ok(())
    }
}

pub fn normalize_login(value: &str) -> String {
    if value.contains('@') {
        value.trim().to_lowercase()
    } else {
        value
            .chars()
            .filter(|c| !c.is_whitespace() && !['(', ')', '-'].contains(c))
            .collect()
    }
}

pub fn valid_login(value: &str) -> bool {
    if value.len() > 254 || value.chars().any(char::is_whitespace) {
        return false;
    }
    if let Some(number) = value.strip_prefix('+') {
        return (10..=15).contains(&number.len())
            && !number.starts_with('0')
            && number.chars().all(|c| c.is_ascii_digit());
    }
    value.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty()
            && domain.contains('.')
            && !domain.contains('@')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComorbiditiesStatus {
    #[default]
    Unknown,
    None,
    Present,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct MedicalHistory {
    pub complaints: String,
    pub present_illness: String,
    pub life_history: String,
    pub comorbidities_status: ComorbiditiesStatus,
    pub comorbidities_details: String,
}

impl MedicalHistory {
    pub fn normalize(&mut self) {
        for text in [
            &mut self.complaints,
            &mut self.present_illness,
            &mut self.life_history,
            &mut self.comorbidities_details,
        ] {
            *text = text.trim().to_owned();
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if [
            &self.complaints,
            &self.present_illness,
            &self.life_history,
            &self.comorbidities_details,
        ]
        .iter()
        .any(|text| {
            text.chars().count() > 6000
                || text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
        }) {
            return Err("Каждое поле анамнеза: до 6000 символов без управляющих знаков");
        }
        if (self.comorbidities_status == ComorbiditiesStatus::Present)
            == self.comorbidities_details.is_empty()
        {
            return Err("Опишите сопутствующие состояния только при выборе «Есть сведения»");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct ProfileInput {
    pub version: i32,
    pub name: String,
    pub birth_date: NaiveDate,
    pub observation_started: NaiveDate,
    #[schema(value_type = PatientStatus)]
    pub status: String,
    pub medical_summary: String,
    pub allergies: Option<String>,
    // Отсутствие блока у старого клиента сохраняет уже внесённый анамнез.
    #[serde(default)]
    pub medical_history: Option<MedicalHistory>,
    #[serde(default)]
    pub physical_info: Option<PhysicalInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct PhysicalInfo {
    #[schema(required = true)]
    pub height_cm: Option<f64>,
    #[schema(required = true)]
    pub weight_kg: Option<f64>,
    #[schema(required = true)]
    pub blood_group: Option<String>,
}

impl ProfileInput {
    pub fn normalize(&mut self) {
        self.name = self.name.trim().to_owned();
        self.medical_summary = self.medical_summary.trim().to_owned();
        if let Some(history) = &mut self.medical_history {
            history.normalize();
        }
        self.allergies = self
            .allergies
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
    }

    pub fn validate(&self, today: NaiveDate) -> Result<(), &'static str> {
        validate_identity(&self.name, self.birth_date, today)?;
        if let Some(history) = &self.medical_history {
            history.validate()?;
        }
        if let Some(info) = &self.physical_info
            && (info
                .height_cm
                .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 400.0)
                || info
                    .weight_kg
                    .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1000.0)
                || info.blood_group.as_deref().is_some_and(|v| {
                    ![
                        "O (I) Rh+",
                        "O (I) Rh−",
                        "A (II) Rh+",
                        "A (II) Rh−",
                        "B (III) Rh+",
                        "B (III) Rh−",
                        "AB (IV) Rh+",
                        "AB (IV) Rh−",
                    ]
                    .contains(&v)
                }))
        {
            return Err("Проверьте рост, вес и группу крови");
        }
        if self.version < 1 {
            return Err("Некорректная версия карточки");
        }
        if self.observation_started > today || self.observation_started < self.birth_date {
            return Err("Начало наблюдения должно быть между датой рождения и сегодняшней датой");
        }
        if !PatientStatus::valid(&self.status) {
            return Err("Неизвестный статус наблюдения");
        }
        if self.medical_summary.chars().count() > 6000
            || self
                .allergies
                .as_ref()
                .is_some_and(|s| s.chars().count() > 2000)
        {
            return Err("Медицинские сведения: до 6000 символов; аллергии: до 2000 символов");
        }
        Ok(())
    }
}

fn validate_identity(
    name: &str,
    birth_date: NaiveDate,
    today: NaiveDate,
) -> Result<(), &'static str> {
    if name.trim().is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err("Имя пациента должно содержать от 1 до 120 символов без управляющих знаков");
    }
    if birth_date > today
        || birth_date < NaiveDate::from_ymd_opt(1900, 1, 1).expect("корректная дата")
    {
        return Err("Проверьте дату рождения пациента");
    }
    Ok(())
}

pub fn validate_new_password(password: &str) -> Result<(), &'static str> {
    if !(8..=128).contains(&password.chars().count()) || password.trim().is_empty() {
        return Err("Пароль должен содержать от 8 до 128 символов");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrospective_events_keep_explicit_time_and_normalize_only_metadata() {
        let now = DateTime::parse_from_rfc3339("2026-10-25T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let birth = NaiveDate::from_ymd_opt(1990, 1, 1).unwrap();
        let mut event = EventInput {
            id: Uuid::new_v4(),
            title: "  Консультация  ".into(),
            body: " Запись врача ".into(),
            patient_visible: true,
            kind: EventKind::Consultation,
            occurred_at: Some((now - chrono::Duration::days(30)).fixed_offset()),
            source: Some("  Выписка от 25.09.2026  ".into()),
        };
        event.normalize();
        assert_eq!(event.title, "Консультация");
        assert_eq!(event.source.as_deref(), Some("Выписка от 25.09.2026"));
        assert_eq!(event.body, " Запись врача ");
        assert!(event.validate(now, birth).is_ok());
        event.occurred_at = Some((now + chrono::Duration::minutes(6)).fixed_offset());
        assert!(event.validate(now, birth).is_err());
        event.occurred_at = Some(
            birth
                .pred_opt()
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_utc()
                .fixed_offset(),
        );
        assert!(event.validate(now, birth).is_err());
        event.occurred_at =
            Some(DateTime::parse_from_rfc3339("1990-01-01T00:00:00+03:00").unwrap());
        assert!(event.validate(now, birth).is_ok());
        event.source = Some("я".repeat(501));
        assert!(event.validate(now, birth).is_err());
    }

    #[test]
    fn legacy_event_inputs_default_to_note_without_inventing_a_source_or_time() {
        let event: EventInput = serde_json::from_str(r#"{"id":"00000000-0000-0000-0000-000000000001","title":"Заметка","body":"","patient_visible":false}"#).unwrap();
        assert_eq!(event.kind, EventKind::Note);
        assert_eq!(event.kind.as_str(), "note");
        assert!(event.occurred_at.is_none());
        assert!(event.source.is_none());
    }

    fn entry() -> DiaryInput {
        DiaryInput {
            id: Uuid::new_v4(),
            measured_at: Utc::now(),
            mood: 0,
            anxiety: 10,
            sleep_hours: 0.0,
            comment: String::new(),
        }
    }

    #[test]
    fn accepts_boundaries_without_turning_zero_into_missing() {
        let mut e = entry();
        assert!(e.validate(Utc::now()).is_ok());
        e.sleep_hours = 24.0;
        assert!(e.validate(Utc::now()).is_ok());
    }

    #[test]
    fn rejects_out_of_range_and_nonfinite_values() {
        let mut e = entry();
        e.mood = 11;
        assert!(e.validate(Utc::now()).is_err());
        e.mood = 5;
        e.sleep_hours = f64::NAN;
        assert!(e.validate(Utc::now()).is_err());
    }

    #[test]
    fn rejects_future_and_overlong_comments() {
        let mut e = entry();
        e.measured_at = Utc::now() + chrono::Duration::hours(1);
        assert!(e.validate(Utc::now()).is_err());
        e.measured_at = Utc::now();
        e.comment = "я".repeat(2001);
        assert!(e.validate(Utc::now()).is_err());
    }

    #[test]
    fn normalizes_identity_and_rejects_invalid_email_and_birth_date() {
        let today = Utc::now().date_naive();
        let mut input = CreatePatientInput {
            id: Uuid::new_v4(),
            name: "  Анна Петрова  ".into(),
            email: "  ANNA@SYNAPSE.DEMO  ".into(),
            birth_date: today,
        };
        input.normalize();
        assert_eq!(input.name, "Анна Петрова");
        assert_eq!(input.email, "anna@synapse.demo");
        assert!(input.validate(today).is_ok());
        input.email = "anna@@example.com".into();
        assert!(input.validate(today).is_err());
        input.email = "anna@example.com".into();
        input.birth_date = today + chrono::Duration::days(1);
        assert!(input.validate(today).is_err());
    }

    #[test]
    fn distinguishes_unknown_allergies_and_rejects_invalid_profile_versions_and_dates() {
        let today = Utc::now().date_naive();
        let mut input = ProfileInput {
            version: 1,
            name: "Анна".into(),
            birth_date: today,
            observation_started: today,
            status: "active".into(),
            medical_summary: String::new(),
            allergies: Some("  ".into()),
            medical_history: None,
            physical_info: None,
        };
        input.normalize();
        assert_eq!(input.allergies, None);
        assert!(input.validate(today).is_ok());
        input.version = 0;
        assert!(input.validate(today).is_err());
        input.version = 1;
        input.observation_started = today - chrono::Duration::days(1);
        assert!(input.validate(today).is_err());
    }

    #[test]
    fn password_limits_count_characters_and_reject_whitespace_only() {
        assert!(validate_new_password("КрепкийПароль").is_ok());
        assert!(validate_new_password("коротко").is_err());
        assert!(validate_new_password(&" ".repeat(10)).is_err());
        assert!(validate_new_password(&"я".repeat(129)).is_err());
    }
}
