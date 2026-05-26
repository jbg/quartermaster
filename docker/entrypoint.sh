#!/bin/sh
set -eu

OPTIONS_FILE="/data/options.json"
APP_USER="quartermaster"

export QM_DATABASE_URL="${QM_DATABASE_URL:-sqlite:///data/data.db?mode=rwc}"

set_from_options() {
	option_name="$1"
	env_name="$2"

	if [ ! -r "$OPTIONS_FILE" ]; then
		return
	fi

	if ! jq -e --arg key "$option_name" 'has($key) and .[$key] != null' "$OPTIONS_FILE" >/dev/null; then
		return
	fi

	value="$(jq -r --arg key "$option_name" '.[$key]' "$OPTIONS_FILE")"
	if [ -n "$value" ]; then
		export "${env_name}=${value}"
	fi
}

set_from_options "public_base_url" "QM_PUBLIC_BASE_URL"
set_from_options "registration_mode" "QM_REGISTRATION_MODE"
set_from_options "expiry_reminders_enabled" "QM_EXPIRY_REMINDERS_ENABLED"
set_from_options "expiry_reminder_lead_days" "QM_EXPIRY_REMINDER_LEAD_DAYS"
set_from_options "expiry_reminder_fire_hour" "QM_EXPIRY_REMINDER_FIRE_HOUR"
set_from_options "expiry_reminder_fire_minute" "QM_EXPIRY_REMINDER_FIRE_MINUTE"
set_from_options "off_credential_encryption_key" "QM_OFF_CREDENTIAL_ENCRYPTION_KEY"
set_from_options "supplier_credential_encryption_key" "QM_SUPPLIER_CREDENTIAL_ENCRYPTION_KEY"
set_from_options "ai_provider" "QM_AI_PROVIDER"
set_from_options "ai_model" "QM_AI_MODEL"
set_from_options "ai_retain_raw_responses" "QM_AI_RETAIN_RAW_RESPONSES"
set_from_options "ai_openrouter_api_key" "QM_AI_OPENROUTER_API_KEY"
set_from_options "ai_openrouter_base_url" "QM_AI_OPENROUTER_BASE_URL"
set_from_options "auth_session_cleanup_interval_seconds" "QM_AUTH_SESSION_CLEANUP_INTERVAL_SECONDS"
set_from_options "expiry_reminder_reconcile_interval_seconds" "QM_EXPIRY_REMINDER_RECONCILE_INTERVAL_SECONDS"
set_from_options "log_format" "QM_LOG_FORMAT"
set_from_options "rust_log" "RUST_LOG"

if [ "$(id -u)" = "0" ]; then
	mkdir -p /data
	chown -R "$APP_USER:$APP_USER" /data
	exec gosu "$APP_USER" "$@"
fi

exec "$@"
