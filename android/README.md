# Quartermaster Android

Jetpack Compose Android client for Quartermaster.

The Android app uses the repo-root `openapi.json` as its single API source of truth. A Gradle `openApiGenerate` task generates the Retrofit/OkHttp client into the app build directory before Kotlin compilation.

## Local development

1. Start the backend from the repo root:

   ```sh
   cargo run -p qm-server
   ```

2. Open the `android/` directory in Android Studio.

3. Let Gradle sync, then run the `app` configuration on an Android emulator.

4. The app defaults to `http://10.0.2.2:8080`. On the standard Android emulator, `10.0.2.2` is a special alias for the host machine's localhost, so this reaches the backend you started on the same computer.

5. Override the server URL from the onboarding screen when:
   - using a physical Android device
   - connecting to Quartermaster on another machine on your LAN
   - connecting to a remote/self-hosted deployment

6. Emulator smoke path:
   - launch the backend with `cargo run -p qm-server`
   - run the Android app in an emulator
   - register or sign in
   - create a household
   - add stock through search or barcode lookup
   - verify inventory and reminders refresh
   - create an invite and open a `quartermaster://join?...` or `/join?...` link to confirm invite handoff

## Generated client

- Input spec: `../openapi.json`
- Generator: `org.openapi.generator`
- Library: Kotlin `jvm-retrofit2`
- JSON: `kotlinx.serialization`

Do not hand-edit generated files under `app/build/generated/`.
