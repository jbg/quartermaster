package dev.quartermaster.android

import android.content.Intent
import android.net.Uri
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class PushSupportTest {
    @Test
    fun `payloadFromMap rejects incomplete payload`() {
        val payload =
            PushSupport.payloadFromMap(
                mapOf(
                    "reminder_id" to "55555555-5555-5555-5555-555555555555",
                    "batch_id" to "33333333-3333-3333-3333-333333333333",
                ),
            )

        assertNull(payload)
    }

    @Test
    fun `applyReminderPayload and payloadFromIntent round trip reminder fields`() {
        val payload = reminderPayload()
        val intent = PushSupport.applyReminderPayload(Intent("test.action.OPEN"), payload)

        assertEquals(payload, PushSupport.payloadFromIntent(intent))
    }

    @Test
    fun `reminder intent router routes deep link and payload once per signature`() = runTest {
        val seenDeepLinks = mutableListOf<Uri>()
        val seenReminderIds = mutableListOf<String>()
        val router =
            ReminderIntentRouter(
                handleDeepLink = seenDeepLinks::add,
                handleIntent = { intent ->
                    PushSupport.payloadFromIntent(intent)?.let { seenReminderIds += it.reminderId }
                },
            )
        val intent =
            PushSupport.applyReminderPayload(
                Intent(Intent.ACTION_VIEW, Uri.parse("quartermaster://join?invite=DEEP1234")),
                reminderPayload(),
            )

        router.route(intent)
        router.route(intent)

        assertEquals(listOf(Uri.parse("quartermaster://join?invite=DEEP1234")), seenDeepLinks)
        assertEquals(listOf("55555555-5555-5555-5555-555555555555"), seenReminderIds)
    }

    @Test
    fun `reminder intent router distinguishes different payloads`() = runTest {
        val seenReminderIds = mutableListOf<String>()
        val router =
            ReminderIntentRouter(
                handleDeepLink = {},
                handleIntent = { intent ->
                    PushSupport.payloadFromIntent(intent)?.let { seenReminderIds += it.reminderId }
                },
            )
        val first = PushSupport.applyReminderPayload(Intent(), reminderPayload())
        val second =
            PushSupport.applyReminderPayload(
                Intent(),
                reminderPayload(reminderId = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"),
            )

        router.route(first)
        router.route(second)

        assertEquals(
            listOf(
                "55555555-5555-5555-5555-555555555555",
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            ),
            seenReminderIds,
        )
    }

    private fun reminderPayload(reminderId: String = "55555555-5555-5555-5555-555555555555") = ReminderPushPayload(
        reminderId = reminderId,
        batchId = "33333333-3333-3333-3333-333333333333",
        productId = "44444444-4444-4444-4444-444444444444",
        locationId = "22222222-2222-2222-2222-222222222222",
        kind = "expiry",
        title = "Milk expires tomorrow",
        body = "Pantry",
    )
}
