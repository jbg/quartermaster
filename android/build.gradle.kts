plugins {
    id("com.android.application") version "9.3.1" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.10" apply false
    id("org.jetbrains.kotlin.plugin.serialization") version "2.4.10" apply false
    id("org.openapi.generator") version "7.23.0" apply false
    id("com.diffplug.spotless") version "8.8.0"
}

allprojects {
    dependencyLocking {
        lockAllConfigurations()
    }
}

spotless {
    kotlin {
        target("app/src/**/*.kt")
        ktlint("1.5.0").editorConfigOverride(
            mapOf(
                "ktlint_standard_max-line-length" to "disabled",
                "ktlint_standard_property-naming" to "disabled",
                "ktlint_standard_function-naming" to "disabled",
            ),
        )
    }
    kotlinGradle {
        target("*.gradle.kts", "app/*.gradle.kts", "settings.gradle.kts")
        ktlint("1.5.0")
    }
}
