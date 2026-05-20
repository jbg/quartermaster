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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    Admin,
    ReadOnly,
    ReadWrite,
}

impl MembershipRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }
}

impl fmt::Display for MembershipRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MembershipRole {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Self::Admin),
            "read_only" => Ok(Self::ReadOnly),
            "read_write" | "member" => Ok(Self::ReadWrite),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown membership role in DB row: {other}",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReminderKind {
    Expiry,
}

impl ReminderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expiry => "expiry",
        }
    }
}

impl fmt::Display for ReminderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReminderKind {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "expiry" => Ok(Self::Expiry),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown reminder kind in DB row: {other}",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReminderUrgency {
    Expired,
    ExpiresToday,
    ExpiresTomorrow,
    ExpiresFuture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LabelPrinterDriver {
    #[serde(rename = "brother_ql_raster")]
    BrotherQlRaster,
}

impl LabelPrinterDriver {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BrotherQlRaster => "brother_ql_raster",
        }
    }
}

impl fmt::Display for LabelPrinterDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LabelPrinterDriver {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "brother_ql_raster" => Ok(Self::BrotherQlRaster),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown label printer driver in DB row: {other}",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LabelPrinterDelivery {
    Server,
    Client,
}

impl LabelPrinterDelivery {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
        }
    }
}

impl fmt::Display for LabelPrinterDelivery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LabelPrinterDelivery {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "server" => Ok(Self::Server),
            "client" => Ok(Self::Client),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown label printer delivery in DB row: {other}",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LabelPrinterMedia {
    #[serde(rename = "dk_62_continuous")]
    Dk62Continuous,
    #[serde(rename = "dk_62_red_black_continuous")]
    Dk62RedBlackContinuous,
    #[serde(rename = "dk_29x90")]
    Dk29x90,
}

impl LabelPrinterMedia {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dk62Continuous => "dk_62_continuous",
            Self::Dk62RedBlackContinuous => "dk_62_red_black_continuous",
            Self::Dk29x90 => "dk_29x90",
        }
    }

    pub const fn is_continuous(self) -> bool {
        matches!(self, Self::Dk62Continuous | Self::Dk62RedBlackContinuous)
    }
}

impl fmt::Display for LabelPrinterMedia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LabelPrinterMedia {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dk_62_continuous" => Ok(Self::Dk62Continuous),
            "dk_62_red_black_continuous" => Ok(Self::Dk62RedBlackContinuous),
            "dk_29x90" => Ok(Self::Dk29x90),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown label printer media in DB row: {other}",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LabelPrintSize {
    Standard,
    Small,
}

impl Default for LabelPrintSize {
    fn default() -> Self {
        Self::Standard
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
