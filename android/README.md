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

7. Emulator smoke path:
   - launch the backend with the reminder env vars above
   - seed or refresh local smoke data
   - run the Android app in an emulator
   - sign in with the seeded smoke account
   - verify inventory and reminders refresh
   - open a reminder and confirm Inventory highlights the related batch
   - create an invite and open a join link to confirm invite handoff
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
   QM_ANDROID_SMOKE_USERNAME=android_smoke_18423 \
   QM_ANDROID_SMOKE_PASSWORD=quartermaster-smoke-18423 \
   ./scripts/smoke_ui.py
   ```

   The script preflights the host backend at `http://127.0.0.1:8080`, runs `adb reverse tcp:8080 tcp:8080`, and rewrites the app's server field to `http://127.0.0.1:8080` inside the emulator. Override the host or device URLs with `QM_ANDROID_SMOKE_HOST_SERVER_URL` and `QM_ANDROID_SMOKE_DEVICE_SERVER_URL` when needed. It also assumes the login already has a household and at least one due reminder so it can verify reminder open → inventory highlight. It clears app data by default; pass `--preserve-app-data` to keep the current emulator session.

   To seed or refresh the local smoke account, stock row, due reminder, and invite code before running the UI driver:

   ```sh
   ./scripts/seed_smoke_data.py
   ```

   This helper assumes:
   - a local backend is already running against the default repo-root `data.db`
   - the backend is using SQLite, not Postgres
   - local smoke setup is allowed to force one reminder row due in SQLite so the inbox path is deterministic

## Verification

From `android/`:

```sh
gradle testDebugUnitTest assembleDebug
```

If generated OpenAPI source wiring changes, verify from a clean app build directory:

```sh
rm -rf app/build
gradle assembleDebug
```

## Generated client

- Input spec: `../openapi.json`
- Generator: `org.openapi.generator`
- Library: Kotlin `jvm-retrofit2`
- JSON: `kotlinx.serialization`
- Output: `app/build/generated/openapi/src/main/kotlin`

Do not hand-edit generated files under `app/build/generated/`.
