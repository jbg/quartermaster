use std::{fmt, future::Future, pin::Pin};

use thiserror::Error;

#[derive(Clone, Debug)]
pub struct EmailAddress {
    pub email: String,
    pub name: Option<String>,
}

impl EmailAddress {
    pub fn new(email: impl Into<String>, name: Option<String>) -> Self {
        Self {
            email: email.into(),
            name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EmailMessage {
    pub to: EmailAddress,
    pub subject: String,
    pub text_body: String,
}

#[derive(Debug, Error)]
#[error("email delivery failed")]
pub struct EmailDeliveryError {
    source: anyhow::Error,
}

impl EmailDeliveryError {
    pub fn new(source: impl Into<anyhow::Error>) -> Self {
        Self {
            source: source.into(),
        }
    }

    pub fn source(&self) -> &anyhow::Error {
        &self.source
    }
}

pub trait EmailTransport: Send + Sync + fmt::Debug {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailDeliveryError>> + Send + 'a>>;
}

#[derive(Debug, Default)]
pub struct LogEmailTransport;

impl EmailTransport for LogEmailTransport {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailDeliveryError>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                to = %message.to.email,
                subject = %message.subject,
                body = %message.text_body,
                "email delivery using explicit log transport"
            );
            Ok(())
        })
    }
}
