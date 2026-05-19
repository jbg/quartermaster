package dev.quartermaster.android

import android.content.Context
import androidx.credentials.CreatePublicKeyCredentialRequest
import androidx.credentials.CreatePublicKeyCredentialResponse
import androidx.credentials.CredentialManager
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetPublicKeyCredentialOption
import androidx.credentials.PublicKeyCredential
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement

internal class AndroidPasskeyCredentialManager(private val context: Context) {
    private val credentialManager = CredentialManager.create(context)

    suspend fun create(publicKeyOptions: JsonElement): JsonElement {
        val response = credentialManager.createCredential(
            context = context,
            request = CreatePublicKeyCredentialRequest(publicKeyOptions.toString()),
        )
        val credential = response as? CreatePublicKeyCredentialResponse
            ?: throw IllegalStateException("Passkey registration returned an unsupported credential.")
        return Json.parseToJsonElement(credential.registrationResponseJson)
    }

    suspend fun get(publicKeyOptions: JsonElement): JsonElement {
        val response = credentialManager.getCredential(
            context = context,
            request = GetCredentialRequest(
                credentialOptions = listOf(GetPublicKeyCredentialOption(publicKeyOptions.toString())),
            ),
        )
        val credential = response.credential as? PublicKeyCredential
            ?: throw IllegalStateException("Passkey sign-in returned an unsupported credential.")
        return Json.parseToJsonElement(credential.authenticationResponseJson)
    }
}
