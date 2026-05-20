package dev.quartermaster.android

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Test
import java.net.ServerSocket

class LabelPrinterClientTest {
    @Test
    fun `tcp sender writes payload to printer socket`() = runTest {
        ServerSocket(0).use { server ->
            val received = async(Dispatchers.IO) {
                server.accept().use { socket ->
                    socket.getInputStream().readBytes()
                }
            }

            val payload = byteArrayOf(0x00, 0x1b, 0x69, 0x5a)
            TcpLabelPrinterSender().send(payload, "127.0.0.1", server.localPort)

            assertArrayEquals(payload, received.await())
        }
    }
}
