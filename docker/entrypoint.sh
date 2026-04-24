#!/bin/sh
set -eu

OPTIONS_FILE="/data/options.json"

export QM_DATABASE_URL="${QM_DATABASE_URL:-sqlite:///data/data.db?mode=rwc}"

set_from_options() {
  option_name="$1"
  env_name="$2"

  if [ ! -f "$OPTIONS_FILE" ]; then
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
set_from_options "auth_session_sweep_interval_seconds" "QM_AUTH_SESSION_SWEEP_INTERVAL_SECONDS"
set_from_options "expiry_reminder_sweep_interval_seconds" "QM_EXPIRY_REMINDER_SWEEP_INTERVAL_SECONDS"
set_from_options "log_format" "QM_LOG_FORMAT"
set_from_options "rust_log" "RUST_LOG"

exec "$@"
