//! Доставка одноразовых кодов регистрации через SMTP или SMS HTTP API.

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationChannel {
    Email,
    Sms,
}

impl VerificationChannel {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Sms => "sms",
        }
    }
}

#[derive(Clone)]
struct SmtpConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    from: String,
    starttls: bool,
}

#[derive(Clone)]
struct SmsConfig {
    url: String,
    token: Option<String>,
}

#[derive(Clone)]
pub struct VerificationDelivery {
    smtp: Option<SmtpConfig>,
    sms: Option<SmsConfig>,
    http: reqwest::Client,
    log_codes: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("канал подтверждения не настроен")]
    NotConfigured,
    #[error("некорректный адрес отправителя или получателя")]
    InvalidAddress,
    #[error("не удалось настроить SMTP")]
    SmtpConfiguration,
    #[error("SMTP не принял сообщение")]
    Smtp,
    #[error("SMS-шлюз не принял сообщение")]
    Sms,
}

impl VerificationDelivery {
    pub fn from_env() -> Self {
        let smtp = non_empty_env("SMTP_HOST").map(|host| SmtpConfig {
            host,
            port: std::env::var("SMTP_PORT")
                .ok()
                .and_then(|raw| raw.parse().ok())
                .unwrap_or(587),
            username: non_empty_env("SMTP_USERNAME"),
            password: non_empty_env("SMTP_PASSWORD"),
            from: std::env::var("SMTP_FROM")
                .unwrap_or_else(|_| "noreply@rent.tou.edu.kz".to_owned()),
            starttls: !std::env::var("SMTP_STARTTLS").is_ok_and(|value| value == "0"),
        });
        let sms = non_empty_env("SMS_GATEWAY_URL").map(|url| SmsConfig {
            url,
            token: non_empty_env("SMS_GATEWAY_TOKEN"),
        });
        let log_codes = std::env::var("VERIFICATION_LOG_CODES")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));

        Self {
            smtp,
            sms,
            http: reqwest::Client::new(),
            log_codes,
        }
    }

    pub async fn send(
        &self,
        channel: VerificationChannel,
        recipient: &str,
        code: &str,
    ) -> Result<(), DeliveryError> {
        match channel {
            VerificationChannel::Email => match &self.smtp {
                Some(config) => send_email(config, recipient, code).await,
                None => self.log_code(channel, code),
            },
            VerificationChannel::Sms => match &self.sms {
                Some(config) => self.send_sms(config, recipient, code).await,
                None => self.log_code(channel, code),
            },
        }
    }

    fn log_code(&self, channel: VerificationChannel, code: &str) -> Result<(), DeliveryError> {
        if !self.log_codes {
            return Err(DeliveryError::NotConfigured);
        }
        tracing::info!(?channel, code, "код подтверждения (только dev-режим)");
        Ok(())
    }

    async fn send_sms(
        &self,
        config: &SmsConfig,
        recipient: &str,
        code: &str,
    ) -> Result<(), DeliveryError> {
        let mut request = self.http.post(&config.url).json(&serde_json::json!({
            "to": recipient,
            "message": format!("TOU.Rent: код подтверждения {code}. Действует 15 минут."),
        }));
        if let Some(token) = config.token.as_deref() {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .await
            .map_err(|_| DeliveryError::Sms)?
            .error_for_status()
            .map_err(|_| DeliveryError::Sms)?;
        Ok(())
    }
}

async fn send_email(config: &SmtpConfig, recipient: &str, code: &str) -> Result<(), DeliveryError> {
    let from: Mailbox = config
        .from
        .parse()
        .map_err(|_| DeliveryError::InvalidAddress)?;
    let to: Mailbox = recipient
        .parse()
        .map_err(|_| DeliveryError::InvalidAddress)?;
    let message = Message::builder()
        .from(from)
        .to(to)
        .subject("Код подтверждения TOU.Rent")
        .body(format!(
            "Код подтверждения: {code}\n\nКод действует 15 минут."
        ))
        .map_err(|_| DeliveryError::InvalidAddress)?;

    let builder = if config.starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
            .map_err(|_| DeliveryError::SmtpConfiguration)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
    }
    .port(config.port);
    let transport = match (&config.username, &config.password) {
        (Some(username), Some(password)) => builder
            .credentials(Credentials::new(username.clone(), password.clone()))
            .build(),
        _ => builder.build(),
    };

    transport
        .send(message)
        .await
        .map_err(|_| DeliveryError::Smtp)?;
    Ok(())
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
