//! Small API-layer enums that ride along on DTOs.
//!
//! Repos continue to read / write these as plain TEXT columns, so we treat
//! this module as a *boundary* type system — strings cross in from the DB,
//! strongly-typed values cross out to the wire, and any unexpected strings
//! produce a domain error rather than a panic.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProductSource {
    Openfoodfacts,
    Manual,
}

impl ProductSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Openfoodfacts => "openfoodfacts",
            Self::Manual => "manual",
        }
    }
}

impl fmt::Display for ProductSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProductSource {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openfoodfacts" => Ok(Self::Openfoodfacts),
            "manual" => Ok(Self::Manual),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown product source in DB row: {other}",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum StockEventType {
    Add,
    Consume,
    Adjust,
    Discard,
    Restore,
}

impl StockEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Consume => "consume",
            Self::Adjust => "adjust",
            Self::Discard => "discard",
            Self::Restore => "restore",
        }
    }
}

impl fmt::Display for StockEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StockEventType {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "add" => Ok(Self::Add),
            "consume" => Ok(Self::Consume),
            "adjust" => Ok(Self::Adjust),
            "discard" => Ok(Self::Discard),
            "restore" => Ok(Self::Restore),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown stock event type in DB row: {other}",
            ))),
        }
    }
}
