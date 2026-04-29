use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    labels::{
        build_label_job, BrotherQlRasterPrinter, BrotherQlRenderer, LabelJob, LabelPrinter,
        LabelRenderer,
    },
    types::{LabelPrinterDriver, LabelPrinterMedia},
    AppState,
};

const ROLE_ADMIN: &str = "admin";
const DEFAULT_BROTHER_PORT: i64 = 9100;
const MAX_COPIES: u8 = 10;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/label-printers",
            get(list_label_printers).post(create_label_printer),
        )
        .route(
            "/label-printers/{id}",
            axum::routing::patch(update_label_printer).delete(delete_label_printer),
        )
        .route("/label-printers/{id}/test", post(test_label_printer))
        .route("/stock/{id}/labels/print", post(print_stock_label))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LabelPrinterDto {
    pub id: Uuid,
    pub name: String,
    pub driver: LabelPrinterDriver,
    pub address: String,
    pub port: i64,
    pub media: LabelPrinterMedia,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LabelPrinterListResponse {
    pub items: Vec<LabelPrinterDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLabelPrinterRequest {
    pub name: String,
    pub driver: LabelPrinterDriver,
    pub address: String,
    pub port: Option<i64>,
    pub media: LabelPrinterMedia,
    pub enabled: Option<bool>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateLabelPrinterRequest {
    pub name: Option<String>,
    pub address: Option<String>,
    pub port: Option<i64>,
    pub media: Option<LabelPrinterMedia>,
    pub enabled: Option<bool>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PrintStockLabelRequest {
    pub printer_id: Option<Uuid>,
    pub copies: Option<u8>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LabelPrintStatus {
    Sent,
    Rendered,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PrintStockLabelResponse {
    pub printer_id: Uuid,
    pub batch_id: Uuid,
    pub batch_url: String,
    pub copies: u8,
    pub status: LabelPrintStatus,
}

#[utoipa::path(
    get,
    path = "/label-printers",
    operation_id = "label_printers_list",
    tag = "label-printers",
    responses((status = 200, body = LabelPrinterListResponse)),
    security(("bearer" = [])),
)]
pub async fn list_label_printers(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<LabelPrinterListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let rows = qm_db::label_printers::list_for_household(&state.db, household_id).await?;
    Ok(Json(LabelPrinterListResponse {
        items: rows.into_iter().map(to_dto).collect::<ApiResult<_>>()?,
    }))
}

#[utoipa::path(
    post,
    path = "/label-printers",
    operation_id = "label_printers_create",
    tag = "label-printers",
    request_body = CreateLabelPrinterRequest,
    responses((status = 201, body = LabelPrinterDto)),
    security(("bearer" = [])),
)]
pub async fn create_label_printer(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateLabelPrinterRequest>,
) -> ApiResult<(StatusCode, Json<LabelPrinterDto>)> {
    require_admin(&current)?;
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let name = validate_name(&req.name)?;
    let address = validate_address(&req.address)?;
    let port = validate_port(req.port.unwrap_or(DEFAULT_BROTHER_PORT))?;
    let row = qm_db::label_printers::create(
        &state.db,
        household_id,
        &qm_db::label_printers::NewLabelPrinter {
            name,
            driver: req.driver.as_str(),
            address,
            port,
            media: req.media.as_str(),
            enabled: req.enabled.unwrap_or(true),
            is_default: req.is_default.unwrap_or(false),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(to_dto(row)?)))
}

#[utoipa::path(
    patch,
    path = "/label-printers/{id}",
    operation_id = "label_printers_update",
    tag = "label-printers",
    params(("id" = Uuid, Path)),
    request_body = UpdateLabelPrinterRequest,
    responses((status = 200, body = LabelPrinterDto)),
    security(("bearer" = [])),
)]
pub async fn update_label_printer(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateLabelPrinterRequest>,
) -> ApiResult<Json<LabelPrinterDto>> {
    require_admin(&current)?;
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let name = req.name.as_deref().map(validate_name).transpose()?;
    let address = req.address.as_deref().map(validate_address).transpose()?;
    let port = req.port.map(validate_port).transpose()?;
    let media = req.media.map(|m| m.to_string());
    let row = qm_db::label_printers::update(
        &state.db,
        household_id,
        id,
        &qm_db::label_printers::LabelPrinterUpdate {
            name,
            address,
            port,
            media: media.as_deref(),
            enabled: req.enabled,
            is_default: req.is_default,
        },
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(to_dto(row)?))
}

#[utoipa::path(
    delete,
    path = "/label-printers/{id}",
    operation_id = "label_printers_delete",
    tag = "label-printers",
    params(("id" = Uuid, Path)),
    responses((status = 204)),
    security(("bearer" = [])),
)]
pub async fn delete_label_printer(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require_admin(&current)?;
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    if !qm_db::label_printers::delete(&state.db, household_id, id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/label-printers/{id}/test",
    operation_id = "label_printers_test",
    tag = "label-printers",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = PrintStockLabelResponse)),
    security(("bearer" = [])),
)]
pub async fn test_label_printer(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PrintStockLabelResponse>> {
    require_admin(&current)?;
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let printer = qm_db::label_printers::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !printer.enabled {
        return Err(ApiError::BadRequest("label printer is disabled".into()));
    }
    let media = LabelPrinterMedia::from_str(&printer.media)?;
    let job = LabelJob {
        batch_id: Uuid::nil(),
        batch_url: state
            .config
            .public_base_url
            .as_deref()
            .unwrap_or("https://quartermaster.invalid")
            .trim_end_matches('/')
            .to_owned(),
        product_name: "Quartermaster test".into(),
        brand: None,
        location_name: "Printer".into(),
        quantity: "1".into(),
        unit: "label".into(),
        expires_on: None,
        opened_on: None,
        note: Some("Test print".into()),
    };
    let rendered = BrotherQlRenderer.render(&job, media)?;
    send_to_printer(&printer, &rendered, 1).await?;
    Ok(Json(PrintStockLabelResponse {
        printer_id: printer.id,
        batch_id: job.batch_id,
        batch_url: job.batch_url,
        copies: 1,
        status: LabelPrintStatus::Sent,
    }))
}

#[utoipa::path(
    post,
    path = "/stock/{id}/labels/print",
    operation_id = "stock_label_print",
    tag = "label-printers",
    params(("id" = Uuid, Path)),
    request_body = PrintStockLabelRequest,
    responses(
        (status = 200, body = PrintStockLabelResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn print_stock_label(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PrintStockLabelRequest>,
) -> ApiResult<Json<PrintStockLabelResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let copies = req.copies.unwrap_or(1);
    if !(1..=MAX_COPIES).contains(&copies) {
        return Err(ApiError::BadRequest(format!(
            "copies must be between 1 and {MAX_COPIES}"
        )));
    }
    let printer = match req.printer_id {
        Some(printer_id) => qm_db::label_printers::find(&state.db, household_id, printer_id)
            .await?
            .ok_or(ApiError::NotFound)?,
        None => qm_db::label_printers::default_enabled(&state.db, household_id)
            .await?
            .ok_or_else(|| ApiError::BadRequest("no enabled label printer is configured".into()))?,
    };
    if !printer.enabled {
        return Err(ApiError::BadRequest("label printer is disabled".into()));
    }
    let job = build_label_job(&state, household_id, id).await?;
    let media = LabelPrinterMedia::from_str(&printer.media)?;
    let rendered = BrotherQlRenderer.render(&job, media)?;
    let status = if req.dry_run.unwrap_or(false) {
        LabelPrintStatus::Rendered
    } else {
        send_to_printer(&printer, &rendered, copies).await?;
        LabelPrintStatus::Sent
    };
    Ok(Json(PrintStockLabelResponse {
        printer_id: printer.id,
        batch_id: id,
        batch_url: job.batch_url,
        copies,
        status,
    }))
}

async fn send_to_printer(
    printer: &qm_db::label_printers::LabelPrinterRow,
    rendered: &crate::labels::RenderedLabel,
    copies: u8,
) -> ApiResult<()> {
    let driver = LabelPrinterDriver::from_str(&printer.driver)?;
    match driver {
        LabelPrinterDriver::BrotherQlRaster => {
            let port = u16::try_from(printer.port)
                .map_err(|_| ApiError::BadRequest("printer port must fit in u16".into()))?;
            let printer = BrotherQlRasterPrinter {
                address: printer.address.clone(),
                port,
            };
            let receipt = printer.print(rendered, copies).await?;
            tracing::info!(bytes_sent = receipt.bytes_sent, "label sent to printer");
            Ok(())
        }
    }
}

fn to_dto(row: qm_db::label_printers::LabelPrinterRow) -> ApiResult<LabelPrinterDto> {
    Ok(LabelPrinterDto {
        id: row.id,
        name: row.name,
        driver: LabelPrinterDriver::from_str(&row.driver)?,
        address: row.address,
        port: row.port,
        media: LabelPrinterMedia::from_str(&row.media)?,
        enabled: row.enabled,
        is_default: row.is_default,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn require_admin(current: &CurrentUser) -> ApiResult<()> {
    if current.role.as_deref() == Some(ROLE_ADMIN) {
        Ok(())
    } else {
        Err(ApiError::AdminOnly)
    }
}

fn validate_name(name: &str) -> ApiResult<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(ApiError::BadRequest(
            "printer name must be 1..=80 chars".into(),
        ));
    }
    Ok(name)
}

fn validate_address(address: &str) -> ApiResult<&str> {
    let address = address.trim();
    if address.is_empty() || address.len() > 255 || address.contains('/') {
        return Err(ApiError::BadRequest(
            "printer address must be a host or IP address".into(),
        ));
    }
    Ok(address)
}

fn validate_port(port: i64) -> ApiResult<i64> {
    if !(1..=65535).contains(&port) {
        return Err(ApiError::BadRequest(
            "printer port must be between 1 and 65535".into(),
        ));
    }
    Ok(port)
}
