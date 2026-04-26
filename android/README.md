# Quartermaster Android

Jetpack Compose Android client for Quartermaster.

The Android app uses the repo-root `openapi.json` as its single API source of truth. A Gradle `openApiGenerate` task generates the Retrofit/OkHttp client into the app build directory before Kotlin compilation.

## Local development

1. Install an Android SDK. The current local setup is known to work with the Homebrew command-line tools:

   ```sh
   brew install --cask android-commandlinetools
   ```

   Then set `sdk.dir` in `android/local.properties`. On Apple Silicon with the Homebrew command-line tools this is typically:

   ```properties
   sdk.dir=/opt/homebrew/share/android-commandlinetools
   ```

   Android Studio's standard `~/Library/Android/sdk` layout also works; point `sdk.dir` at whichever SDK root owns your installed platforms/build tools.

2. Start the backend from the repo root:

   ```sh
   cargo run -p qm-server
   ```

   For reminder smoke testing, use expiry reminders and choose a household-local fire time that is already due:

   ```sh
   QM_EXPIRY_REMINDERS_ENABLED=true \
   QM_EXPIRY_REMINDER_FIRE_HOUR=9 \
   QM_EXPIRY_REMINDER_FIRE_MINUTE=0 \
   cargo run -p qm-server
   ```

3. Open the `android/` directory in Android Studio.

4. Let Gradle sync, then run the `app` configuration on an Android emulator.

5. The app defaults to `http://10.0.2.2:8080`. On the standard Android emulator, `10.0.2.2` is a special alias for the host machine's localhost, so this reaches the backend you started on the same computer. Debug builds allow cleartext traffic to this local endpoint.

6. Override the server URL from the onboarding screen when:
   - using a physical Android device
   - connecting to Quartermaster on another machine on your LAN
   - connecting to a remote/self-hosted deployment

7. Android push reminders use env-driven Firebase config. Set these before building if you want FCM enabled in the app:

   ```sh
   export QUARTERMASTER_ANDROID_FIREBASE_PROJECT_ID=...
   export QUARTERMASTER_ANDROID_FIREBASE_APPLICATION_ID=...
   export QUARTERMASTER_ANDROID_FIREBASE_API_KEY=...
   export QUARTERMASTER_ANDROID_FIREBASE_SENDER_ID=...
   ```

   The app does not require a tracked `google-services.json`. If those env vars are unset, the Android client still works, but reminder delivery stays inbox-only.

8. Emulator smoke path:
   - launch the backend with the reminder env vars above
   - seed or refresh local smoke data
   - run the Android app in an emulator
   - sign in with the seeded smoke account
   - verify inventory and reminders refresh
   - acknowledge one due reminder and confirm it disappears from the inbox
   - open a second due reminder and confirm Inventory highlights the related batch
   - dismiss the reminder banner, create an invite, and open a join link to confirm invite handoff
   - sign out, then sign back in and confirm session recovery

   To open a custom-scheme invite link in the emulator:

   ```sh
   adb shell am start \
     -a android.intent.action.VIEW \
     -d "quartermaster://join?invite=CODE&server=http%3A%2F%2F10.0.2.2%3A8080"
   ```

   To smoke the browser fallback shape, use the same command with an HTTP(S) `/join` URL:

   ```sh
   adb shell am start \
     -a android.intent.action.VIEW \
     -d "http://10.0.2.2:8080/join?invite=CODE&server=http%3A%2F%2F10.0.2.2%3A8080"
   ```

   For repeatable UI smoke testing, prefer the UIAutomator driver over manual taps. It finds controls from the accessibility tree by text/label and taps their actual bounds:

   ```sh
   QM_ANDROID_SMOKE_MAINTENANCE_TOKEN=... \
   ./scripts/smoke_ui.py
   ```

   The script preflights the host backend at `http://127.0.0.1:8080`, runs `adb reverse tcp:8080 tcp:8080`, and rewrites the app's server field to `http://127.0.0.1:8080` inside the emulator. Override the host or device URLs with `QM_ANDROID_SMOKE_HOST_SERVER_URL` and `QM_ANDROID_SMOKE_DEVICE_SERVER_URL` when needed. When `QM_ANDROID_SMOKE_MAINTENANCE_TOKEN` is set, the script first calls the backend-owned `POST /internal/maintenance/seed-android-smoke` fixture route to create or refresh the smoke user, due reminders, and invite code before it verifies reminder acknowledge + notification-open → inventory highlight. It clears app data by default; pass `--preserve-app-data` to keep the current emulator session. You can still supply `QM_ANDROID_SMOKE_USERNAME` / `QM_ANDROID_SMOKE_PASSWORD` manually if you want to skip the fixture route.

   The default run is the full end-to-end path. For faster local retries, pass `--flow` with one of `reminders`, `inventory`, `products`, `locations`, or `invite-session`. The selected flow still performs the same backend preflight, fixture seeding when a maintenance token is present, `adb reverse`, app launch, and sign-in. The `inventory` flow requires fixture data from `QM_ANDROID_SMOKE_MAINTENANCE_TOKEN`.

   To seed or refresh the local smoke account, due reminders, and invite code without launching the UI driver:

   ```sh
   QM_ANDROID_SMOKE_MAINTENANCE_TOKEN=... \
   ./scripts/seed_smoke_data.py
   ```

   This helper assumes:
   - a local backend is already running
   - `QM_ANDROID_SMOKE_SEED_TRIGGER_SECRET` is set on the backend and the same value is supplied as `QM_ANDROID_SMOKE_MAINTENANCE_TOKEN`
   - the backend fixture route seeds two due reminders so the smoke driver can cover both acknowledge and notification-open flows in one run

## Verification

From `android/`:

```sh
gradle testDebugUnitTest assembleDebug
```

This is a host-only check. Run it from a normal macOS shell with a working JDK and Android SDK, not from inside the Codex sandbox.

Expected local environment:

- Java 17 available on `PATH`
- `sdk.dir` configured in `android/local.properties`
- Android SDK rooted at either `/opt/homebrew/share/android-commandlinetools` or `~/Library/Android/sdk`

If `gradle testDebugUnitTest` fails in the sandbox with `Failed to load native library 'libnative-platform.dylib' for Mac OS X aarch64`, rerun it on the host before changing Gradle config. The host run is the source of truth for this repo.

If generated OpenAPI source wiring changes, verify from a clean app build directory:

```sh
rm -rf app/build
gradle assembleDebug
```

The emulator smoke path is also host-only because it depends on `adb`, a running emulator, and host backend access outside the sandbox.

## Generated client

- Input spec: `../openapi.json`
- Generator: `org.openapi.generator`
- Library: Kotlin `jvm-retrofit2`
- JSON: `kotlinx.serialization`
- Output: `app/build/generated/openapi/src/main/kotlin`

Do not hand-edit generated files under `app/build/generated/`.
