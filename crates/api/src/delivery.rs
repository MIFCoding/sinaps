use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use serde::Serialize;
use std::{env, sync::Arc, time::Duration};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeMode {
    Demo,
    Strict,
}

#[async_trait]
pub trait MailSender: Send + Sync {
    async fn send(&self, recipient: &str, subject: &str, body: &str) -> Result<(), String>;
}

#[async_trait]
pub trait PushSender: Send + Sync {
    async fn send(&self, message: PushMessage) -> Result<(), String>;
}

#[derive(Serialize)]
pub struct PushMessage {
    pub notification_id: String,
    pub recipient_id: String,
    pub kind: String,
    pub created_at: String,
}

pub struct Delivery {
    mode: RuntimeMode,
    mailer: Option<Arc<dyn MailSender>>,
    push: Option<Arc<dyn PushSender>>,
    public_app_url: Option<String>,
}

impl Delivery {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let mode = match env::var("SYNAPSE_RUNTIME_MODE").as_deref() {
            Err(_) | Ok("demo") => RuntimeMode::Demo,
            Ok("strict") => RuntimeMode::Strict,
            Ok(_) => return Err("SYNAPSE_RUNTIME_MODE должен быть demo или strict".into()),
        };
        let mail_transport = setting("SYNAPSE_MAIL_TRANSPORT", mode)?;
        let push_transport = setting("SYNAPSE_PUSH_TRANSPORT", mode)?;
        if mode == RuntimeMode::Strict
            && (mail_transport.as_deref() != Some("smtp")
                || push_transport.as_deref() != Some("webhook"))
        {
            return Err("Строгий режим требует SYNAPSE_MAIL_TRANSPORT=smtp и SYNAPSE_PUSH_TRANSPORT=webhook".into());
        }

        let public_app_url = nonempty("SYNAPSE_PUBLIC_APP_URL")?;
        if public_app_url
            .as_deref()
            .is_some_and(|url| !url.starts_with("http://") && !url.starts_with("https://"))
        {
            return Err("SYNAPSE_PUBLIC_APP_URL должен начинаться с http:// или https://".into());
        }
        let mailer = match mail_transport.as_deref() {
            None | Some("disabled") => None,
            Some("smtp") => {
                let host = required("SYNAPSE_SMTP_HOST")?;
                let port = required("SYNAPSE_SMTP_PORT")?.parse::<u16>()?;
                let sender = required("SYNAPSE_SMTP_FROM")?.parse::<Mailbox>()?;
                let tls = required("SYNAPSE_SMTP_TLS")?;
                let username = required("SYNAPSE_SMTP_USERNAME")?;
                let password = required("SYNAPSE_SMTP_PASSWORD")?;
                let builder = match tls.as_str() {
                    "implicit" => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?,
                    "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)?,
                    _ => return Err("SYNAPSE_SMTP_TLS должен быть implicit или starttls".into()),
                };
                let transport = builder
                    .port(port)
                    .credentials(Credentials::new(username, password))
                    .timeout(Some(Duration::from_secs(10)))
                    .build();
                Some(Arc::new(SmtpMailer { transport, sender }) as Arc<dyn MailSender>)
            }
            Some(_) => {
                return Err("SYNAPSE_MAIL_TRANSPORT поддерживает только smtp или disabled".into());
            }
        };
        let push = match push_transport.as_deref() {
            None | Some("disabled") => None,
            Some("webhook") => {
                let url = required("SYNAPSE_PUSH_WEBHOOK_URL")?;
                if mode == RuntimeMode::Strict && !url.starts_with("https://") {
                    return Err(
                        "В строгом режиме SYNAPSE_PUSH_WEBHOOK_URL должен использовать HTTPS"
                            .into(),
                    );
                }
                let token = required("SYNAPSE_PUSH_WEBHOOK_TOKEN")?;
                Some(Arc::new(WebhookPush {
                    client: reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()?,
                    url,
                    token,
                }) as Arc<dyn PushSender>)
            }
            Some(_) => {
                return Err(
                    "SYNAPSE_PUSH_TRANSPORT поддерживает только webhook или disabled".into(),
                );
            }
        };
        if mailer.is_some() && public_app_url.is_none() {
            return Err("Почтовый транспорт требует SYNAPSE_PUBLIC_APP_URL".into());
        }
        if mode == RuntimeMode::Strict && public_app_url.as_deref().is_none() {
            return Err("Строгий режим требует SYNAPSE_PUBLIC_APP_URL".into());
        }
        if mode == RuntimeMode::Strict
            && !public_app_url
                .as_deref()
                .is_some_and(|url| url.starts_with("https://"))
        {
            return Err("В строгом режиме SYNAPSE_PUBLIC_APP_URL должен использовать HTTPS".into());
        }
        Ok(Self {
            mode,
            mailer,
            push,
            public_app_url,
        })
    }

    pub fn is_strict(&self) -> bool {
        self.mode == RuntimeMode::Strict
    }

    pub fn has_mail(&self) -> bool {
        self.mailer.is_some()
    }

    pub fn has_push(&self) -> bool {
        self.push.is_some()
    }

    pub fn confirmation_url(&self, token: &str) -> Option<String> {
        self.public_app_url
            .as_ref()
            .map(|base| format!("{}/#verify={token}", base.trim_end_matches('/')))
    }

    pub fn reset_url(&self, token: &str) -> Option<String> {
        self.public_app_url
            .as_ref()
            .map(|base| format!("{}/#reset={token}", base.trim_end_matches('/')))
    }

    pub async fn send_mail(
        &self,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), String> {
        match &self.mailer {
            Some(mailer) => mailer.send(recipient, subject, body).await,
            None => Err("Почтовый транспорт не настроен".into()),
        }
    }

    pub async fn send_push(&self, message: PushMessage) -> Result<(), String> {
        match &self.push {
            Some(push) => push.send(message).await,
            None => Ok(()),
        }
    }
}

struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    sender: Mailbox,
}

#[async_trait]
impl MailSender for SmtpMailer {
    async fn send(&self, recipient: &str, subject: &str, body: &str) -> Result<(), String> {
        let recipient = recipient
            .parse::<Mailbox>()
            .map_err(|_| "Неверный адрес получателя".to_owned())?;
        let message = Message::builder()
            .from(self.sender.clone())
            .to(recipient)
            .subject(subject)
            .body(body.to_owned())
            .map_err(|_| "Не удалось подготовить письмо".to_owned())?;
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|_| "SMTP-сервер не принял письмо".to_owned())
    }
}

struct WebhookPush {
    client: reqwest::Client,
    url: String,
    token: String,
}

#[async_trait]
impl PushSender for WebhookPush {
    async fn send(&self, message: PushMessage) -> Result<(), String> {
        self.client
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&message)
            .send()
            .await
            .map_err(|_| "Push-шлюз недоступен".to_owned())?
            .error_for_status()
            .map(|_| ())
            .map_err(|_| "Push-шлюз отклонил уведомление".to_owned())
    }
}

fn setting(name: &str, mode: RuntimeMode) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => {
            if mode == RuntimeMode::Strict {
                Err(format!("{name} должен содержать значение").into())
            } else {
                Ok(None)
            }
        }
        Ok(value) => Ok(Some(value)),
        Err(_) if mode == RuntimeMode::Strict => {
            Err(format!("{name} должен быть явно задан в строгом режиме").into())
        }
        Err(_) => Ok(None),
    }
}

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    nonempty(name)?.ok_or_else(|| format!("{name} должен быть явно задан").into())
}

fn nonempty(name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}
