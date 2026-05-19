package dev.quartermaster.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AuthHandoffLinkTest {
    @Test
    fun `parseAuthHandoff accepts handoff payload`() {
        val parsed = QuartermasterAppState.parseAuthHandoff(
            "quartermaster://handoff?server=https%3A%2F%2Fqm.example.com&id=handoff-123&token=secret-456",
        )

        assertEquals("handoff-123", parsed?.id)
        assertEquals("secret-456", parsed?.token)
        assertEquals("https://qm.example.com", parsed?.serverUrl)
        assertNull(parsed?.preview)
    }

    @Test
    fun `parseAuthHandoff rejects setup and incomplete links`() {
        assertNull(QuartermasterAppState.parseAuthHandoff("quartermaster://server?server=https://qm.example.com"))
        assertNull(QuartermasterAppState.parseAuthHandoff("quartermaster://handoff?id=handoff-123"))
        assertNull(QuartermasterAppState.parseAuthHandoff("quartermaster://handoff?token=secret-456"))
    }
}
