package dev.quartermaster.android

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.InetSocketAddress
import java.net.Socket

interface LabelPrinterSender {
    suspend fun send(payload: ByteArray, address: String, port: Int)
}

class TcpLabelPrinterSender : LabelPrinterSender {
    override suspend fun send(payload: ByteArray, address: String, port: Int) {
        withContext(Dispatchers.IO) {
            Socket().use { socket ->
                socket.connect(InetSocketAddress(address, port), CONNECT_TIMEOUT_MS)
                socket.soTimeout = WRITE_TIMEOUT_MS
                socket.getOutputStream().use { output ->
                    output.write(payload)
                    output.flush()
                }
            }
        }
    }

    private companion object {
        const val CONNECT_TIMEOUT_MS = 5_000
        const val WRITE_TIMEOUT_MS = 5_000
    }
}
