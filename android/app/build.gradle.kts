import org.jetbrains.kotlin.gradle.tasks.KotlinCompile
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.openapitools.generator.gradle.plugin.tasks.GenerateTask

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("org.openapi.generator")
}

android {
    namespace = "dev.quartermaster.android"
    compileSdk {
        version = release(37) {
            minorApiLevel = 0
        }
    }

    defaultConfig {
        applicationId = "dev.quartermaster.android"
        minSdk = 28
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"

        buildConfigField("String", "FIREBASE_PROJECT_ID", "\"${System.getenv("QUARTERMASTER_ANDROID_FIREBASE_PROJECT_ID") ?: ""}\"")
        buildConfigField("String", "FIREBASE_APPLICATION_ID", "\"${System.getenv("QUARTERMASTER_ANDROID_FIREBASE_APPLICATION_ID") ?: ""}\"")
        buildConfigField("String", "FIREBASE_API_KEY", "\"${System.getenv("QUARTERMASTER_ANDROID_FIREBASE_API_KEY") ?: ""}\"")
        buildConfigField("String", "FIREBASE_SENDER_ID", "\"${System.getenv("QUARTERMASTER_ANDROID_FIREBASE_SENDER_ID") ?: ""}\"")

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables.useSupportLibrary = true
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

androidComponents {
    onVariants { variant ->
        variant.sources.kotlin?.addStaticSourceDirectory(
            layout.buildDirectory.dir("generated/openapi/src/main/kotlin").get().asFile.absolutePath
        )
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

openApiGenerate {
    generatorName.set("kotlin")
    inputSpec.set(rootProject.file("../openapi.json").absolutePath)
    outputDir.set(layout.buildDirectory.dir("generated/openapi").get().asFile.absolutePath)
    packageName.set("dev.quartermaster.android.generated")
    apiPackage.set("dev.quartermaster.android.generated.api")
    modelPackage.set("dev.quartermaster.android.generated.models")
    invokerPackage.set("dev.quartermaster.android.generated.infrastructure")
    library.set("jvm-retrofit2")
    configOptions.set(
        mapOf(
            "serializationLibrary" to "kotlinx_serialization",
            "useCoroutines" to "true",
            "dateLibrary" to "java8",
            "enumPropertyNaming" to "UPPERCASE",
            "useNonAsciiHeaders" to "false",
            "parcelizeModels" to "false",
            "omitGradleWrapper" to "true",
        )
    )
    globalProperties.set(
        mapOf(
            "modelDocs" to "false",
            "apiDocs" to "false",
            "modelTests" to "false",
            "apiTests" to "false",
        )
    )
}

tasks.withType<GenerateTask>().configureEach {
    doLast {
        delete(layout.buildDirectory.dir("generated/openapi/.openapi-generator").get().asFile)
    }
}

tasks.withType<KotlinCompile>().configureEach {
    dependsOn(tasks.named("openApiGenerate"))
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.10.01")
    val firebaseBom = platform("com.google.firebase:firebase-bom:34.4.0")

    implementation(composeBom)
    androidTestImplementation(composeBom)
    implementation(firebaseBom)

    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.navigation:navigation-compose:2.8.4")
    implementation("androidx.security:security-crypto:1.1.0-alpha06")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.core:core-splashscreen:1.0.1")

    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    implementation("com.google.firebase:firebase-messaging")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("com.squareup.okhttp3:logging-interceptor:4.12.0")
    implementation("com.squareup.retrofit2:retrofit:2.11.0")
    implementation("com.squareup.retrofit2:converter-kotlinx-serialization:2.11.0")
    implementation("com.squareup.retrofit2:converter-scalars:2.11.0")

    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
    testImplementation("androidx.arch.core:core-testing:2.2.0")

    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.6.1")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
}
