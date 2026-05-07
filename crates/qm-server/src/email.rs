use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::Context;
use lettre::{
    message::{Mailbox, SinglePart},
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use qm_api::email::{EmailDeliveryError, EmailMessage, EmailTransport, LogEmailTransport};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct EmailTransportConfig {
    pub transport: Option<String>,
    pub from: Option<String>,
    pub from_name: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_tls_mode: String,
    pub jmap_session_url: Option<String>,
    pub jmap_account_id: Option<String>,
    pub jmap_identity_id: Option<String>,
    pub jmap_bearer_token: Option<String>,
    pub http: reqwest::Client,
}

pub fn build_email_transport(
    config: EmailTransportConfig,
) -> anyhow::Result<Option<Arc<dyn EmailTransport>>> {
    let Some(transport) = config
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    match transport {
        "log" => Ok(Some(Arc::new(LogEmailTransport))),
        "smtp" => Ok(Some(Arc::new(SmtpEmailTransport::new(&config)?))),
        "jmap" => Ok(Some(Arc::new(JmapEmailTransport::new(config)?))),
        other => {
            anyhow::bail!("QM_EMAIL_TRANSPORT must be one of log, smtp, or jmap (got {other})")
        }
    }
}

#[derive(Debug)]
struct SmtpEmailTransport {
    from: Mailbox,
    inner: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpEmailTransport {
    fn new(config: &EmailTransportConfig) -> anyhow::Result<Self> {
        let from = parse_mailbox(
            require(&config.from, "QM_EMAIL_FROM")?,
            config.from_name.clone(),
        )?;
        let host = require(&config.smtp_host, "QM_SMTP_HOST")?;
        let port = config
            .smtp_port
            .unwrap_or(match config.smtp_tls_mode.as_str() {
                "implicit" => 465,
                _ => 587,
            });
        let mut builder = match config.smtp_tls_mode.as_str() {
            "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .context("building SMTP STARTTLS transport")?,
            "implicit" => AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .context("building SMTP TLS transport")?,
            "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
            other => {
                anyhow::bail!("QM_SMTP_TLS_MODE must be starttls, implicit, or none (got {other})")
            }
        }
        .port(port);
        if config.smtp_tls_mode == "starttls" {
            let tls = TlsParameters::new(host.to_owned()).context("building SMTP TLS params")?;
            builder = builder.tls(Tls::Required(tls));
        }
        if let Some(username) = config.smtp_username.as_deref() {
            let password = require(&config.smtp_password, "QM_SMTP_PASSWORD")?;
            builder =
                builder.credentials(Credentials::new(username.to_owned(), password.to_owned()));
        }
        Ok(Self {
            from,
            inner: builder.build(),
        })
    }
}

impl EmailTransport for SmtpEmailTransport {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailDeliveryError>> + Send + 'a>> {
        Box::pin(async move {
            let email = Message::builder()
                .from(self.from.clone())
                .to(parse_mailbox(&message.to.email, message.to.name.clone())
                    .map_err(EmailDeliveryError::new)?)
                .subject(message.subject)
                .singlepart(SinglePart::plain(message.text_body))
                .map_err(EmailDeliveryError::new)?;
            self.inner
                .send(email)
                .await
                .map(|_| ())
                .map_err(EmailDeliveryError::new)
        })
    }
}

#[derive(Debug)]
struct JmapEmailTransport {
    from_email: String,
    from_name: Option<String>,
    session_url: String,
    account_id: String,
    identity_id: String,
    bearer_token: String,
    http: reqwest::Client,
}

impl JmapEmailTransport {
    fn new(config: EmailTransportConfig) -> anyhow::Result<Self> {
        Ok(Self {
            from_email: require(&config.from, "QM_EMAIL_FROM")?.to_owned(),
            from_name: config.from_name,
            session_url: require(&config.jmap_session_url, "QM_JMAP_SESSION_URL")?.to_owned(),
            account_id: require(&config.jmap_account_id, "QM_JMAP_ACCOUNT_ID")?.to_owned(),
            identity_id: require(&config.jmap_identity_id, "QM_JMAP_IDENTITY_ID")?.to_owned(),
            bearer_token: require(&config.jmap_bearer_token, "QM_JMAP_BEARER_TOKEN")?.to_owned(),
            http: config.http,
        })
    }
}

impl EmailTransport for JmapEmailTransport {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailDeliveryError>> + Send + 'a>> {
        Box::pin(async move {
            let session: JmapSession = self
                .http
                .get(&self.session_url)
                .bearer_auth(&self.bearer_token)
                .send()
                .await
                .map_err(EmailDeliveryError::new)?
                .error_for_status()
                .map_err(EmailDeliveryError::new)?
                .json()
                .await
                .map_err(EmailDeliveryError::new)?;
            let payload = JmapRequest::email_submission(
                &self.account_id,
                &self.identity_id,
                JmapAddress {
                    email: self.from_email.clone(),
                    name: self.from_name.clone(),
                },
                JmapAddress {
                    email: message.to.email,
                    name: message.to.name,
                },
                message.subject,
                message.text_body,
            );
            self.http
                .post(session.api_url)
                .bearer_auth(&self.bearer_token)
                .json(&payload)
                .send()
                .await
                .map_err(EmailDeliveryError::new)?
                .error_for_status()
                .map_err(EmailDeliveryError::new)?;
            Ok(())
        })
    }
}

#[derive(Debug, Deserialize)]
struct JmapSession {
    #[serde(rename = "apiUrl")]
    api_url: String,
}

#[derive(Clone, Debug, Serialize)]
struct JmapAddress {
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct JmapRequest {
    #[serde(rename = "using")]
    using_capabilities: Vec<&'static str>,
    #[serde(rename = "methodCalls")]
    method_calls: Vec<serde_json::Value>,
}

impl JmapRequest {
    fn email_submission(
        account_id: &str,
        identity_id: &str,
        from: JmapAddress,
        to: JmapAddress,
        subject: String,
        text_body: String,
    ) -> Self {
        Self {
            using_capabilities: vec![
                "urn:ietf:params:jmap:core",
                "urn:ietf:params:jmap:mail",
                "urn:ietf:params:jmap:submission",
            ],
            method_calls: vec![
                serde_json::json!([
                    "Email/set",
                    {
                        "accountId": account_id,
                        "create": {
                            "reset": {
                                "mailboxIds": {},
                                "keywords": { "$draft": true },
                                "from": [from],
                                "to": [to],
                                "subject": subject,
                                "textBody": [{ "partId": "text" }],
                                "bodyValues": { "text": { "value": text_body } }
                            }
                        }
                    },
                    "email"
                ]),
                serde_json::json!([
                    "EmailSubmission/set",
                    {
                        "accountId": account_id,
                        "create": {
                            "reset": {
                                "emailId": "#reset",
                                "identityId": identity_id
                            }
                        },
                        "onSuccessDestroyEmail": ["#reset"]
                    },
                    "submission"
                ]),
            ],
        }
    }
}

fn require<'a>(value: &'a Option<String>, name: &str) -> anyhow::Result<&'a str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("{name} is required"))
}

fn parse_mailbox(email: &str, name: Option<String>) -> anyhow::Result<Mailbox> {
    Ok(Mailbox::new(
        name.filter(|s| !s.trim().is_empty()),
        email.parse().context("parsing email address")?,
    ))
}
