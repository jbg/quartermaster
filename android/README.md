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

4. The app defaults to `http://10.0.2.2:8080`, which reaches the host machine from the Android emulator. Override the server URL from the onboarding screen when connecting to another self-hosted instance.

## Generated client

- Input spec: `../openapi.json`
- Generator: `org.openapi.generator`
- Library: Kotlin `jvm-retrofit2`
- JSON: `kotlinx.serialization`

Do not hand-edit generated files under `app/build/generated/`.
