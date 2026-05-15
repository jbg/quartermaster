use uuid::Uuid;

use crate::{ApiError, ApiResult, AppState};

pub async fn ensure_can_add_household_for_current_account(
    state: &AppState,
    current_household_id: Option<Uuid>,
) -> ApiResult<Option<Uuid>> {
    let Some(maximum) = state.config.plan_limits.households_per_billing_account else {
        return Ok(None);
    };
    let Some(household_id) = current_household_id else {
        return Ok(None);
    };
    let account = qm_db::billing::ensure_for_household(
        &state.db,
        household_id,
        qm_db::billing::DEFAULT_PLAN_KEY,
    )
    .await?;
    let current = qm_db::billing::count_households(&state.db, account.id).await?;
    enforce("households_per_billing_account", current, maximum)?;
    Ok(Some(account.id))
}

pub async fn ensure_can_add_member(state: &AppState, household_id: Uuid) -> ApiResult<()> {
    enforce_optional(
        "members_per_household",
        qm_db::quotas::count_household_members(&state.db, household_id).await?,
        state.config.plan_limits.members_per_household,
    )
}

pub async fn ensure_can_add_product(state: &AppState, household_id: Uuid) -> ApiResult<()> {
    enforce_optional(
        "products_per_household",
        qm_db::quotas::count_household_products(&state.db, household_id).await?,
        state.config.plan_limits.products_per_household,
    )
}

pub async fn ensure_can_add_stock_batch(
    state: &AppState,
    household_id: Uuid,
    will_create_reminder: bool,
) -> ApiResult<()> {
    enforce_optional(
        "stock_batches_per_household",
        qm_db::quotas::count_household_stock_batches(&state.db, household_id).await?,
        state.config.plan_limits.stock_batches_per_household,
    )?;
    if will_create_reminder {
        ensure_can_add_reminder(state, household_id).await?;
    }
    Ok(())
}

pub async fn ensure_can_add_reminder(state: &AppState, household_id: Uuid) -> ApiResult<()> {
    enforce_optional(
        "reminders_per_household",
        qm_db::quotas::count_household_reminders(&state.db, household_id).await?,
        state.config.plan_limits.reminders_per_household,
    )
}

pub async fn ensure_can_add_invite(state: &AppState, household_id: Uuid) -> ApiResult<()> {
    enforce_optional(
        "invites_per_household",
        qm_db::quotas::count_household_invites(&state.db, household_id).await?,
        state.config.plan_limits.invites_per_household,
    )
}

pub async fn ensure_can_add_push_device(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    enforce_optional(
        "push_devices_per_user",
        qm_db::quotas::count_user_push_devices(&state.db, user_id).await?,
        state.config.plan_limits.push_devices_per_user,
    )
}

fn enforce_optional(limit: &'static str, current: i64, maximum: Option<i64>) -> ApiResult<()> {
    if let Some(maximum) = maximum {
        enforce(limit, current, maximum)?;
    }
    Ok(())
}

fn enforce(limit: &'static str, current: i64, maximum: i64) -> ApiResult<()> {
    if maximum >= 0 && current >= maximum {
        return Err(ApiError::PlanLimitExceeded { limit, maximum });
    }
    Ok(())
}
